//! The T-001 workload: one parse of every tracked TypeScript file with oxc, then three rule shapes
//! taken from the reference implementation, built once and run in-process, inside a WebAssembly
//! component and inside a separate process.
//!
//! - `walk`: a full visitor over every syntax tree, counting decorated classes and properties (the
//!   shape of the module, field and declaration inventories);
//! - `semantic`: scopes, symbols and references of every source file below `src/` (the shape of the
//!   role and configuration rules that read bindings);
//! - `resolve`: every import and re-export specifier resolved with `oxc_resolver` and the tsconfig
//!   `paths`, then the import cycles of the module graph (the shape of the import and dependency
//!   rules).
//!
//! The tree is read only through [`Tree`], so the same code runs over the file system, over host
//! calls and over a pipe. Every outcome carries counts, so the three forms prove they did the same
//! work.

pub mod frame;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{Class, Program, PropertyDefinition, Statement};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_resolver::{
    EnforceExtension, FileMetadata, FileSystem, ResolveError, ResolveOptions, ResolverGeneric,
    TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType};
use serde::{Deserialize, Serialize};

/// The extensions of the TypeScript files the reference implementation parses.
pub const EXTENSIONS: [&str; 4] = ["ts", "tsx", "mts", "cts"];

/// The virtual root the resolver sees; no form touches a real path through it.
const ROOT: &str = "/repo";

/// Read-only access to one tracked tree.
pub trait Tree {
    /// Every tracked path, slash-separated, relative to the root.
    fn files(&mut self) -> Vec<String>;
    /// The bytes of one tracked file; `None` when it is not tracked.
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
}

/// What a run asks for.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Request {
    /// Any of `read`, `parse`, `walk`, `semantic`, `resolve`; every task after `read` parses first.
    pub tasks: Vec<String>,
}

/// What a run did: the time of each phase in milliseconds, measured inside the form, and the counts
/// that show the forms did the same work.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Outcome {
    pub phases: BTreeMap<String, f64>,
    pub counts: BTreeMap<String, u64>,
}

impl Outcome {
    fn phase(&mut self, name: &str, started: Instant) {
        *self.phases.entry(name.to_owned()).or_default() += started.elapsed().as_secs_f64() * 1e3;
    }

    fn count(&mut self, name: &str, value: usize) {
        *self.counts.entry(name.to_owned()).or_default() += value as u64;
    }
}

/// Whether `path` is a TypeScript file the reference implementation parses.
#[must_use]
pub fn is_typescript(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension))
}

/// One parsed file; its tree lives in the run's allocator.
struct Parsed<'a> {
    path: String,
    source: &'a str,
    program: &'a Program<'a>,
    fatal: bool,
}

/// Runs the tasks of `request` over `tree`.
pub fn run(tree: &mut dyn Tree, request: &Request) -> Outcome {
    let mut outcome = Outcome::default();
    let wants = |task: &str| request.tasks.iter().any(|name| name == task);

    let started = Instant::now();
    let files: Vec<String> = tree.files();
    let typescript: Vec<&String> = files.iter().filter(|path| is_typescript(path)).collect();
    outcome.phase("list", started);
    outcome.count("tracked", files.len());
    outcome.count("typescript", typescript.len());

    if wants("read") {
        let started = Instant::now();
        let mut bytes = 0;
        for path in &typescript {
            bytes += tree.read(path).map_or(0, |content| content.len());
        }
        outcome.phase("read", started);
        outcome.count("bytes", bytes);
    }

    if !["parse", "walk", "semantic", "resolve"].iter().any(|task| wants(task)) {
        return outcome;
    }

    let allocator = Allocator::default();
    let parsed = parse_all(tree, &typescript, &allocator, &mut outcome);

    if wants("walk") {
        let started = Instant::now();
        let mut inventory = Inventory::default();
        for file in parsed.iter().filter(|file| !file.fatal) {
            inventory.visit_program(file.program);
        }
        outcome.phase("walk", started);
        outcome.count("nodes", inventory.nodes);
        outcome.count("classes", inventory.classes);
        outcome.count("class_decorators", inventory.class_decorators);
        outcome.count("property_decorators", inventory.property_decorators);
    }

    if wants("semantic") {
        let started = Instant::now();
        let (mut files, mut symbols, mut unresolved) = (0, 0, 0);
        for file in parsed.iter().filter(|file| {
            !file.fatal && file.path.starts_with("src/") && !file.path.ends_with(".d.ts")
        }) {
            let semantic = SemanticBuilder::new().build(file.program).semantic;
            files += 1;
            symbols += semantic.scoping().symbols_len();
            unresolved += semantic.scoping().root_unresolved_references().len();
        }
        outcome.phase("semantic", started);
        outcome.count("semantic_files", files);
        outcome.count("symbols", symbols);
        outcome.count("unresolved_names", unresolved);
    }

    if wants("resolve") {
        let started = Instant::now();
        resolve_all(tree, &files, &parsed, &mut outcome);
        outcome.phase("resolve", started);
    }

    outcome
}

fn parse_all<'a>(
    tree: &mut dyn Tree,
    typescript: &[&String],
    allocator: &'a Allocator,
    outcome: &mut Outcome,
) -> Vec<Parsed<'a>> {
    let mut parsed = Vec::with_capacity(typescript.len());
    let (mut fatal, mut bytes) = (0, 0);

    for path in typescript {
        let started = Instant::now();
        let content = tree.read(path).unwrap_or_default();
        outcome.phase("parse.read", started);

        let started = Instant::now();
        bytes += content.len();
        let text = std::str::from_utf8(&content).unwrap_or("");
        let source = allocator.alloc_str(text);
        let source_type = SourceType::from_path(path.as_str()).unwrap_or_default();
        let result = Parser::new(allocator, source, source_type).parse();
        fatal += usize::from(result.fatal_error);
        parsed.push(Parsed {
            path: (*path).clone(),
            source,
            fatal: result.fatal_error,
            program: allocator.alloc(result.program),
        });
        outcome.phase("parse.oxc", started);
    }

    outcome.count("parsed_bytes", bytes);
    outcome.count("fatal", fatal);
    outcome.count("allocator_bytes", allocator.used_bytes());
    parsed
}

/// The decorator inventory of the `walk` rule.
#[derive(Default)]
struct Inventory {
    nodes: usize,
    classes: usize,
    class_decorators: usize,
    property_decorators: usize,
}

impl<'a> Visit<'a> for Inventory {
    fn enter_node(&mut self, _kind: AstKind<'a>) {
        self.nodes += 1;
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        self.classes += 1;
        self.class_decorators += class.decorators.len();
        walk::walk_class(self, class);
    }

    fn visit_property_definition(&mut self, property: &PropertyDefinition<'a>) {
        self.property_decorators += property.decorators.len();
        walk::walk_property_definition(self, property);
    }
}

// ---------------------------------------------------------------------------------------------
// resolve: the module graph and its cycles.

/// The extensions and aliases of the reference resolver (the compiler's bundler mode).
const RESOLVE_EXTENSIONS: [&str; 5] = [".ts", ".tsx", ".d.ts", ".js", ".jsx"];
const EXTENSION_ALIASES: [(&str, &[&str]); 9] = [
    (".cjs", &[".cts", ".d.cts", ".cjs"]),
    (".cts", &[".cts", ".d.cts", ".cjs"]),
    (".js", &[".ts", ".tsx", ".d.ts", ".js", ".jsx"]),
    (".json", &[".d.json.ts", ".json"]),
    (".jsx", &[".tsx", ".ts", ".d.ts", ".jsx", ".js"]),
    (".mjs", &[".mts", ".d.mts", ".mjs"]),
    (".mts", &[".mts", ".d.mts", ".mjs"]),
    (".ts", &[".ts", ".tsx", ".d.ts", ".js", ".jsx"]),
    (".tsx", &[".tsx", ".ts", ".d.ts", ".jsx", ".js"]),
];

fn resolve_all(tree: &mut dyn Tree, files: &[String], parsed: &[Parsed<'_>], outcome: &mut Outcome) {
    let root = PathBuf::from(ROOT);
    let file_system = TrackedFileSystem::load(tree, files, &root);
    let tsconfig = files.iter().any(|path| path == "tsconfig.json").then(|| {
        TsconfigDiscovery::Manual(TsconfigOptions {
            config_file: root.join("tsconfig.json"),
            references: TsconfigReferences::Disabled,
        })
    });
    let options = ResolveOptions {
        cwd: Some(root.clone()),
        tsconfig,
        condition_names: ["import", "types"].map(str::to_owned).to_vec(),
        enforce_extension: EnforceExtension::Enabled,
        extension_alias: EXTENSION_ALIASES
            .iter()
            .map(|(extension, aliases)| {
                (
                    (*extension).to_owned(),
                    aliases.iter().map(|alias| (*alias).to_owned()).collect(),
                )
            })
            .collect(),
        extensions: RESOLVE_EXTENSIONS.map(str::to_owned).to_vec(),
        main_fields: ["typings", "types", "main"].map(str::to_owned).to_vec(),
        node_path: false,
        symlinks: false,
        ..ResolveOptions::default()
    };
    let resolver = ResolverGeneric::new_with_file_system(file_system, options);

    let index: HashMap<&str, usize> = parsed
        .iter()
        .enumerate()
        .map(|(position, file)| (file.path.as_str(), position))
        .collect();
    let mut edges = vec![Vec::new(); parsed.len()];
    let (mut specifiers, mut resolved, mut failed) = (0, 0, 0);

    for (position, file) in parsed.iter().enumerate().filter(|(_, file)| !file.fatal) {
        let directory = root
            .join(&file.path)
            .parent()
            .map_or_else(|| root.clone(), Path::to_path_buf);
        for statement in &file.program.body {
            let specifier = match statement {
                Statement::ImportDeclaration(declaration) => Some(&declaration.source),
                Statement::ExportFromDeclaration(declaration) => Some(&declaration.source),
                Statement::ExportAllDeclaration(declaration) => Some(&declaration.source),
                _ => None,
            };
            let Some(specifier) = specifier else { continue };
            specifiers += 1;
            match resolver.resolve(&directory, specifier.value.as_str()) {
                Ok(resolution) => {
                    let target = normalize(resolution.path());
                    let target = target
                        .strip_prefix(&root)
                        .ok()
                        .and_then(|path| path.to_str())
                        .and_then(|path| index.get(path));
                    if let Some(target) = target {
                        resolved += 1;
                        edges[position].push(*target);
                    }
                }
                Err(_) => failed += 1,
            }
        }
    }

    outcome.count("specifiers", specifiers);
    outcome.count("resolved", resolved);
    outcome.count("unresolved_specifiers", failed);
    let cycles = cycles(&edges);
    outcome.count("cycles", cycles.0);
    outcome.count("files_in_cycles", cycles.1);
}

/// The strongly connected components with more than one file, and the files in them (iterative
/// Tarjan, so a deep graph does not overflow a small guest stack).
fn cycles(edges: &[Vec<usize>]) -> (usize, usize) {
    let count = edges.len();
    let mut index = vec![usize::MAX; count];
    let mut low = vec![0; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut next = 0;
    let (mut components, mut members) = (0, 0);

    for start in 0..count {
        if index[start] != usize::MAX {
            continue;
        }
        let mut work = vec![(start, 0)];
        while let Some(&mut (node, ref mut edge)) = work.last_mut() {
            if *edge == 0 && index[node] == usize::MAX {
                index[node] = next;
                low[node] = next;
                next += 1;
                stack.push(node);
                on_stack[node] = true;
            }
            if let Some(&target) = edges[node].get(*edge) {
                *edge += 1;
                if index[target] == usize::MAX {
                    work.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(index[target]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                let mut size = 0;
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    size += 1;
                    if member == node {
                        break;
                    }
                }
                if size > 1 {
                    components += 1;
                    members += size;
                }
            }
        }
    }

    (components, members)
}

/// The tracked files as the resolver sees them, as the reference implementation builds it: every
/// tracked path and its ancestors exist, and the configuration files have their contents.
#[derive(Debug, Default)]
struct TrackedFileSystem {
    files: HashSet<PathBuf>,
    directories: HashSet<PathBuf>,
    contents: HashMap<PathBuf, Vec<u8>>,
}

impl TrackedFileSystem {
    fn load(tree: &mut dyn Tree, files: &[String], root: &Path) -> Self {
        let mut file_system = Self::default();
        file_system.directories.insert(root.to_path_buf());
        for path in files {
            let full = root.join(path);
            if is_configuration(path)
                && let Some(bytes) = tree.read(path)
            {
                file_system.contents.insert(full.clone(), bytes);
            }
            let mut directory = full.parent();
            while let Some(parent) = directory {
                if !parent.starts_with(root) || !file_system.directories.insert(parent.into()) {
                    break;
                }
                directory = parent.parent();
            }
            file_system.files.insert(full);
        }
        file_system
    }

    fn metadata_of(&self, path: &Path) -> io::Result<FileMetadata> {
        let path = normalize(path);
        if self.files.contains(&path) {
            Ok(FileMetadata::new(true, false, false))
        } else if self.directories.contains(&path) {
            Ok(FileMetadata::new(false, true, false))
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
}

fn is_configuration(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == "package.json"
        || ((name.starts_with("tsconfig") || name.starts_with("jsconfig"))
            && name.ends_with(".json"))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

impl FileSystem for TrackedFileSystem {
    fn new() -> Self {
        Self::default()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize(path);
        self.contents
            .get(&path)
            .cloned()
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.metadata_of(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.metadata_of(path)
    }

    fn read_link(&self, path: &Path) -> Result<PathBuf, ResolveError> {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a symbolic link", path.display()),
        )
        .into())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.metadata_of(path).map(|_| normalize(path))
    }
}

// ---------------------------------------------------------------------------------------------
// The cross-boundary shape: a flat node table one plugin builds and another reads.

/// Parses every TypeScript file and returns the flat node table of all of them: per file its path,
/// its source and one `(type, start, end, parent)` record of four little-endian `u32` per node.
pub fn table(tree: &mut dyn Tree, outcome: &mut Outcome) -> Vec<u8> {
    let files = tree.files();
    let typescript: Vec<&String> = files.iter().filter(|path| is_typescript(path)).collect();
    let allocator = Allocator::default();
    let parsed = parse_all(tree, &typescript, &allocator, outcome);

    let started = Instant::now();
    let mut out = Vec::new();
    let mut table = Table::default();
    out.extend_from_slice(&u32_of(parsed.len()).to_le_bytes());
    for file in &parsed {
        table.nodes.clear();
        table.stack.clear();
        if !file.fatal {
            table.visit_program(file.program);
        }
        put_bytes(&mut out, file.path.as_bytes());
        put_bytes(&mut out, file.source.as_bytes());
        out.extend_from_slice(&u32_of(table.nodes.len() / 4).to_le_bytes());
        for value in &table.nodes {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    outcome.phase("table.build", started);
    outcome.count("table_bytes", out.len());
    out
}

/// Counts the decorators on classes and on properties in a flat node table; the counts equal those
/// of the `walk` rule over the syntax trees.
pub fn table_rule(table: &[u8], outcome: &mut Outcome) {
    let started = Instant::now();
    let class = u32::from(AstType::Class as u8);
    let property = u32::from(AstType::PropertyDefinition as u8);
    let decorator = u32::from(AstType::Decorator as u8);
    let (mut nodes, mut class_decorators, mut property_decorators, mut source_bytes) = (0, 0, 0, 0);
    let mut reader = Reader { bytes: table, at: 0 };
    let files = reader.u32();
    for _ in 0..files {
        let _path = reader.bytes();
        source_bytes += reader.bytes().len();
        let count = reader.u32() as usize;
        let records = reader.take(count * 16);
        let kind = |node: usize| u32_at(records, node * 16);
        for node in 0..count {
            nodes += 1;
            if kind(node) == decorator {
                let parent = u32_at(records, node * 16 + 12) as usize;
                let parent_kind = kind(parent);
                if parent_kind == class {
                    class_decorators += 1;
                } else if parent_kind == property {
                    property_decorators += 1;
                }
            }
        }
    }
    outcome.phase("table.rule", started);
    outcome.count("nodes", nodes);
    outcome.count("class_decorators", class_decorators);
    outcome.count("property_decorators", property_decorators);
    outcome.count("table_source_bytes", source_bytes);
}

use oxc_ast::AstType;

#[derive(Default)]
struct Table {
    nodes: Vec<u32>,
    stack: Vec<u32>,
}

impl<'a> Visit<'a> for Table {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        let position = u32_of(self.nodes.len() / 4);
        let span = kind.span();
        let parent = self.stack.last().copied().unwrap_or(position);
        self.nodes
            .extend([u32::from(kind.ty() as u8), span.start, span.end, parent]);
        self.stack.push(position);
    }

    fn leave_node(&mut self, _kind: AstKind<'a>) {
        self.stack.pop();
    }
}

fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&u32_of(bytes.len()).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut word = [0; 4];
    word.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(word)
}

struct Reader<'b> {
    bytes: &'b [u8],
    at: usize,
}

impl<'b> Reader<'b> {
    fn take(&mut self, length: usize) -> &'b [u8] {
        let slice = &self.bytes[self.at..self.at + length];
        self.at += length;
        slice
    }

    fn u32(&mut self) -> u32 {
        u32_at(self.take(4), 0)
    }

    fn bytes(&mut self) -> &'b [u8] {
        let length = self.u32() as usize;
        self.take(length)
    }
}

// ---------------------------------------------------------------------------------------------
// The tree over a real directory, for the native and process forms and the WASI-preopen guest.

/// A tree read from a directory: the listing is given (the host lists the tracked files, as git
/// does today), the bytes come from the file system.
#[derive(Debug)]
pub struct DirectoryTree {
    pub root: PathBuf,
    pub listing: Vec<String>,
}

impl Tree for DirectoryTree {
    fn files(&mut self) -> Vec<String> {
        self.listing.clone()
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(path)).ok()
    }
}

/// Every file below `root` outside `.git`, slash-separated and sorted: the export holds tracked
/// files only.
///
/// # Errors
/// Fails when a directory cannot be read.
pub fn list_directory(root: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in std::fs::read_dir(root.join(&relative))? {
            let entry = entry?;
            let name = relative.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                // A git checkout of the tree (the hook case) keeps its repository out of the listing.
                if entry.file_name() != ".git" {
                    pending.push(name);
                }
            } else if let Some(text) = name.to_str() {
                out.push(text.replace('\\', "/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Memory(Vec<(String, String)>);

    impl Tree for Memory {
        fn files(&mut self) -> Vec<String> {
            self.0.iter().map(|(path, _)| path.clone()).collect()
        }

        fn read(&mut self, path: &str) -> Option<Vec<u8>> {
            self.0
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, text)| text.clone().into_bytes())
        }
    }

    fn tree() -> Memory {
        Memory(vec![
            (
                "src/a.ts".into(),
                "import { b } from './b';\n@Thing() export class A { @Field() x = b; @Field() y = 1; }"
                    .into(),
            ),
            ("src/b.ts".into(), "import { A } from './a';\nexport const b = 1; A;".into()),
            ("src/c.ts".into(), "export * from './missing';".into()),
            ("tsconfig.json".into(), "{\"compilerOptions\":{}}".into()),
        ])
    }

    #[test]
    fn the_table_rule_counts_what_the_walk_counts() {
        let walked = run(
            &mut tree(),
            &Request { tasks: vec!["walk".into(), "resolve".into(), "semantic".into()] },
        );
        assert_eq!(walked.counts["class_decorators"], 1);
        assert_eq!(walked.counts["property_decorators"], 2);
        assert_eq!(walked.counts["cycles"], 1);
        assert_eq!(walked.counts["files_in_cycles"], 2);
        assert_eq!(walked.counts["resolved"], 2);
        assert_eq!(walked.counts["unresolved_specifiers"], 1);

        let mut outcome = Outcome::default();
        let bytes = table(&mut tree(), &mut outcome);
        let mut read = Outcome::default();
        table_rule(&bytes, &mut read);
        for key in ["nodes", "class_decorators", "property_decorators"] {
            assert_eq!(read.counts[key], walked.counts[key], "{key}");
        }
    }
}

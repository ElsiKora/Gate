//! Module specifier resolution with tsconfig `paths` over the tracked files of a source.
//!
//! The resolver sees the source as a virtual file system that mirrors the repository at
//! [`REPOSITORY`]: every tracked path of the project exists below the project's root, and so does
//! each tsconfig base the `extends` chain names, read through the source with its project-relative
//! path (`../../tsconfig.base.json`). Nothing else exists, and no real file is read.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use oxc_resolver::{
    EnforceExtension, ExtendsField, FileMetadata, FileSystem, ResolveError, ResolveOptions,
    ResolverGeneric, TsConfig, TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};

use crate::source::has_extension;
use crate::{Error, Result, Source};

/// Where the virtual file system of the resolver mounts the repository. The project's tracked files
/// lie below it at the project's root, so a path that leaves the project names the repository path
/// the compiler reads: a base `../../tsconfig.base.json`, or a `paths` target of that base,
/// `apps/api/src/*`, that leads back into the project. Only the project's tracked files and the
/// bases of the `extends` chain exist, so a `..` that leaves the project for any other file
/// (`../../src/a` from `src/app.ts`, a `paths` target `../shared/*`) reaches nothing, as it reached
/// no tracked file of the project before. Error texts name paths relative to the project root.
const REPOSITORY: &str = "/gate/repository";

/// The UTF-8 byte order mark a tsconfig may start with, which oxc reads as whitespace.
const BYTE_ORDER_MARK: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The extensions a specifier without a TypeScript-known extension is tried with, in the order of
/// the TypeScript compiler's `tryAddingExtensions`.
const EXTENSIONS: [&str; 5] = [".ts", ".tsx", ".d.ts", ".js", ".jsx"];

/// For a specifier with a known extension, the files the compiler tries instead, in its order: the
/// extension is replaced, never loaded as written first.
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

/// A declaration extension of a specifier and the source extension whose files the compiler tries
/// instead: it drops `.d.ts`, `.d.mts` or `.d.cts` as it drops any extension it knows.
const DECLARATION_EXTENSIONS: [(&str, &str); 3] =
    [(".d.ts", ".ts"), (".d.mts", ".mts"), (".d.cts", ".cts")];

/// Resolves module specifiers the way `ts.resolveModuleName` does for a tsconfig that sets
/// `moduleResolution` `bundler` with `module` `ESNext`: relative paths, then
/// the tsconfig `paths`, then the package's own
/// `imports` and self-referencing `exports` with the `import`, `types` and `default` conditions, then
/// `node_modules`. A specifier with a declaration extension stands first for its source extension
/// ([`DECLARATION_EXTENSIONS`]) and then for itself, a specifier with a known extension tries the
/// [`EXTENSION_ALIASES`] of that extension, and any specifier then tries [`EXTENSIONS`] appended and the directory
/// (`typings`, `types` or `main` of its `package.json`, then `index`); a file is never loaded
/// without one of those extensions.
///
/// Only tracked files exist for the resolver, so a specifier that the compiler would resolve into
/// an untracked `node_modules` package, outside the project or through a differently cased path
/// resolves to nothing; the compiler reports those as external or absent too, except for case. A
/// tracked file below a `node_modules` directory resolves to nothing as well, as rules treat
/// the compiler's external library imports.
///
/// Of the tsconfig only `paths`, `baseUrl`, `rootDirs` and `extends` are read: the conditions and
/// the extensions are those of bundler mode, so the resolver serves only the configured
/// tsconfig. oxc also differs from the compiler in cases no tested source needs: an `exports` or `imports` target or folder mapping without a known extension gains
/// extensions or an `index`, an array target does not fall through to its next entry, and
/// `typesVersions` and versioned `types@` conditions are ignored.
///
/// A tsconfig base named by a relative path is read through the source with its path relative to
/// the project root, so a base outside the project is read when the source grants it. A base that
/// cannot be read that way, one named by an absolute path or by an empty string, and a `#` base
/// whose `package.json` `imports` target is a file the source cannot read, are dropped from the
/// `extends` the resolver reads and listed by [`Resolver::unreadable`]. A base from a package
/// (`@tsconfig/node22`), or a `#` base that maps to one, is dropped without being listed unless
/// the package is tracked, as an import of an untracked package resolves to nothing.
pub struct Resolver {
    root: PathBuf,
    inner: ResolverGeneric<SourceFileSystem>,
    unreadable: Vec<UnreadableBase>,
}

/// A tsconfig base of the `extends` chain the resolver could not read through the source; it
/// was dropped from the `extends` the resolver reads, so the `paths` and `baseUrl` it sets were
/// not applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnreadableBase {
    /// The tsconfig whose `extends` names the base, relative to the project root (`..` above it).
    pub tsconfig: String,
    /// The `extends` entry as the tsconfig writes it.
    pub extends: String,
    /// The file the compiler would read for the base, relative to the project root (`..` above
    /// it); `None` for an absolute path, an empty entry or a `#` entry.
    pub path: Option<String>,
    /// The base relative to the repository root; `None` when it lies outside the repository.
    pub repository: Option<String>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Resolver")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Resolver {
    /// A resolver for the tracked files of `source`, the files of the project at `project`
    /// (relative to the repository; empty for the repository), with the tracked tsconfig at
    /// `tsconfig` (relative to the project root), or without `paths` when there is none.
    ///
    /// # Errors
    /// Fails when a configuration file cannot be read.
    pub(crate) fn new(source: &dyn Source, project: &str, tsconfig: Option<&str>) -> Result<Self> {
        let mut root = PathBuf::from(REPOSITORY);
        root.extend(
            project
                .split('/')
                .filter(|part| !part.is_empty() && *part != "."),
        );
        let mut file_system = SourceFileSystem::load(source, &root)?;
        let unreadable = match tsconfig {
            Some(tsconfig) => file_system.read_chain(source, &root, tsconfig)?,
            None => Vec::new(),
        };
        let tsconfig = tsconfig.map(|path| {
            TsconfigDiscovery::Manual(TsconfigOptions {
                config_file: root.join(path),
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
            extensions: EXTENSIONS.map(str::to_owned).to_vec(),
            main_fields: ["typings", "types", "main"].map(str::to_owned).to_vec(),
            node_path: false,
            symlinks: false,
            ..ResolveOptions::default()
        };

        Ok(Self {
            inner: ResolverGeneric::new_with_file_system(file_system, options),
            root,
            unreadable,
        })
    }

    /// The bases of the `extends` chain that could not be read, in the order the chain names
    /// them.
    #[must_use]
    pub(crate) fn unreadable(&self) -> &[UnreadableBase] {
        &self.unreadable
    }

    /// `text` with every path of the virtual file system written relative to the project root:
    /// the root's own files without a prefix, a file above it with one `../` per level.
    fn relative_text(&self, text: &str) -> String {
        let mut text = text.replace(&format!("{}/", self.root.display()), "");
        let mut up = String::new();
        let mut directory = self.root.parent();
        while let Some(parent) = directory.filter(|parent| parent.starts_with(REPOSITORY)) {
            up.push_str("../");
            text = text.replace(&format!("{}/", parent.display()), &up);
            directory = parent.parent();
        }
        text
    }

    /// The tracked file `specifier` names when it is imported from the tracked file `importer`.
    ///
    /// # Errors
    /// Fails when the tsconfig or a `package.json` on the way cannot be read or parsed.
    pub(crate) fn resolve(&self, importer: &str, specifier: &str) -> Result<Option<String>> {
        let directory = self
            .root
            .join(importer)
            .parent()
            .map_or_else(|| self.root.clone(), Path::to_path_buf);
        // A declaration specifier names its source file first; an exact `paths`, `imports` or
        // `exports` key that ends in the declaration extension still matches as written.
        let source = DECLARATION_EXTENSIONS
            .iter()
            .find_map(|(declaration, source)| {
                specifier
                    .strip_suffix(declaration)
                    .map(|stem| format!("{stem}{source}"))
            });

        if let Some(source) = source
            && let Some(resolved) = self.resolve_from(&directory, importer, &source)?
        {
            return Ok(Some(resolved));
        }

        self.resolve_from(&directory, importer, specifier)
    }

    fn resolve_from(
        &self,
        directory: &Path,
        importer: &str,
        specifier: &str,
    ) -> Result<Option<String>> {
        match self.inner.resolve(directory, specifier) {
            // A `paths` target keeps the `..` of its substitution, which the compiler normalizes.
            Ok(resolution) => Ok(normalize(resolution.path())
                .strip_prefix(&self.root)
                .ok()
                .and_then(relative_path)
                .filter(|path| !path.split('/').any(|segment| segment == "node_modules"))),
            Err(
                error @ (ResolveError::TsconfigNotFound(_)
                | ResolveError::TsconfigSelfReference(_)
                | ResolveError::TsconfigCircularExtend(_)
                | ResolveError::TsconfigLoadFailed { .. }
                | ResolveError::IOError(_)
                | ResolveError::Json(_)),
            ) => Err(Error::new(format!(
                "cannot resolve {specifier:?} from {importer}: {}",
                self.relative_text(&error.to_string())
            ))),
            Err(_) => Ok(None),
        }
    }
}

/// A slash-separated relative path, when every component is a normal name.
fn relative_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            _ => return None,
        }
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The tracked files of a source as the resolver sees them: every tracked path exists, every
/// ancestor directory of one exists, and the configuration files the resolver reads (each
/// `package.json`, `tsconfig*.json` and `jsconfig*.json`) have their tracked contents.
#[derive(Clone, Debug, Default)]
struct SourceFileSystem {
    files: HashSet<PathBuf>,
    directories: HashSet<PathBuf>,
    contents: HashMap<PathBuf, Vec<u8>>,
}

impl SourceFileSystem {
    fn load(source: &dyn Source, root: &Path) -> Result<Self> {
        let mut file_system = Self::default();
        file_system.directories.insert(root.to_path_buf());

        for path in source.paths() {
            // A granted path outside the project is listed with `..`; only the bases of the
            // `extends` chain exist outside the project.
            let full = normalize(&root.join(path));
            if !full.starts_with(root) {
                continue;
            }
            if is_configuration(path)
                && let Some(bytes) = source.bytes(path)?
            {
                file_system.contents.insert(full.clone(), bytes.to_vec());
            }
            file_system.add_file(full);
        }

        Ok(file_system)
    }

    /// Adds the file `full` and every directory above it.
    fn add_file(&mut self, full: PathBuf) {
        let mut directory = full.parent();
        while let Some(parent) = directory {
            if !self.directories.insert(parent.into()) {
                break;
            }
            directory = parent.parent();
        }
        self.files.insert(full);
    }

    /// Adds the file `full` with `contents`.
    fn add_contents(&mut self, full: PathBuf, contents: &[u8]) {
        self.contents.insert(full.clone(), contents.to_vec());
        self.add_file(full);
    }

    /// Reads the `extends` chain of the tsconfig `tsconfig` (relative to the project root `root`)
    /// through `source`: each base a relative path names is read with its path relative to the
    /// project root, and a `#` or package base is kept when it resolves to a file the resolver
    /// holds. A base that cannot be read is dropped from the `extends` the resolver reads, so
    /// resolution goes on without it, and is answered, except a base from a package that is not
    /// tracked, which is dropped without an answer.
    ///
    /// # Errors
    /// When the source cannot read a base it tracks.
    fn read_chain(
        &mut self,
        source: &dyn Source,
        root: &Path,
        tsconfig: &str,
    ) -> Result<Vec<UnreadableBase>> {
        let mut unreadable = Vec::new();
        let mut seen = HashSet::new();
        let mut queue = vec![tsconfig.to_owned()];
        let mut packages: Option<ResolverGeneric<Self>> = None;

        while let Some(config) = queue.pop() {
            if !seen.insert(config.clone()) {
                continue;
            }
            let full = normalize(&root.join(&config));
            // A configured tsconfig whose name is not one of a configuration file is not loaded.
            if !self.contents.contains_key(&full)
                && let Some(bytes) = source.bytes(&config)?
            {
                self.add_contents(full.clone(), &bytes);
            }
            // A tsconfig the resolver cannot parse is left to oxc, which reports it.
            let Some(extends) = self
                .contents
                .get(&full)
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .and_then(|text| TsConfig::parse(false, &full, &full, text).ok())
                .and_then(|parsed| parsed.extends)
            else {
                continue;
            };
            let entries = match extends {
                ExtendsField::Single(entry) => vec![entry],
                ExtendsField::Multiple(entries) => entries,
            };
            let directory = config
                .rsplit_once('/')
                .map_or("", |(directory, _)| directory);
            let parent = full
                .parent()
                .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
            let mut bases = Vec::new();
            let mut kept = Vec::new();

            for entry in &entries {
                let base = match entry.as_bytes().first() {
                    Some(b'.') => {
                        let target = join_relative(directory, entry);
                        self.read_base(source, root, &target)?.ok_or_else(|| {
                            let file = self.expected_file(root, &target);
                            UnreadableBase {
                                repository: repository_path(root, &file),
                                tsconfig: config.clone(),
                                extends: entry.clone(),
                                path: Some(file),
                            }
                        })
                    }
                    Some(b'#' | b'@' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') => {
                        let resolver = packages.get_or_insert_with(|| {
                            ResolverGeneric::new_with_file_system(self.clone(), extends_options())
                        });
                        match self.read_resolved(source, root, resolver, &parent, entry)? {
                            Some(base) => Ok(base),
                            // An untracked package, or a `#` entry that maps to one, is skipped as
                            // an import of an untracked package resolves to nothing.
                            None if !entry.starts_with('#')
                                || self.imports_package(&parent, entry) =>
                            {
                                continue;
                            }
                            None => Err(UnreadableBase {
                                tsconfig: config.clone(),
                                extends: entry.clone(),
                                path: None,
                                repository: None,
                            }),
                        }
                    }
                    // An empty entry, or an absolute path, which the plugin never reads.
                    _ => Err(UnreadableBase {
                        tsconfig: config.clone(),
                        extends: entry.clone(),
                        path: None,
                        repository: None,
                    }),
                };
                match base {
                    Ok(base) => {
                        bases.push(base);
                        kept.push(entry.clone());
                    }
                    Err(base) => unreadable.push(base),
                }
            }
            if kept.len() < entries.len() {
                self.drop_bases(&full, &kept);
            }
            // The queue is a stack: the chain's first base is read first.
            queue.extend(bases.into_iter().rev());
        }

        Ok(unreadable)
    }

    /// The base the `#` or package `extends` entry `entry` of a tsconfig in the directory `parent`
    /// names, as oxc resolves it with `resolver`, relative to the project root `root`, with its
    /// contents loaded; `None` when it resolves to no file the resolver holds.
    ///
    /// # Errors
    /// When the source cannot read a file it tracks.
    fn read_resolved(
        &mut self,
        source: &dyn Source,
        root: &Path,
        resolver: &ResolverGeneric<Self>,
        parent: &Path,
        entry: &str,
    ) -> Result<Option<String>> {
        let Ok(resolution) = resolver.resolve(parent, entry) else {
            return Ok(None);
        };
        let full = normalize(resolution.path());
        let Some(relative) = full.strip_prefix(root).ok().and_then(relative_path) else {
            return Ok(None);
        };
        if !self.contents.contains_key(&full) {
            match source.bytes(&relative)? {
                Some(bytes) => self.add_contents(full, &bytes),
                None => return Ok(None),
            }
        }
        Ok(Some(relative))
    }

    /// Whether the `#` entry `entry` maps, in the `imports` of the nearest `package.json` above the
    /// directory `parent`, only to packages: no target is a relative path.
    fn imports_package(&self, parent: &Path, entry: &str) -> bool {
        let Some(manifest) = parent
            .ancestors()
            .find_map(|directory| self.contents.get(&directory.join("package.json")))
        else {
            return false;
        };
        let Some(target) = serde_json::from_slice::<serde_json::Value>(manifest)
            .ok()
            .and_then(|manifest| manifest.get("imports")?.get(entry).cloned())
        else {
            return false;
        };
        let mut targets = Vec::new();
        strings(&target, &mut targets);
        !targets.is_empty() && targets.iter().all(|target| !target.starts_with('.'))
    }

    /// The file the compiler reads for the relative `extends` target `target` (relative to the
    /// project root `root`), as a path from the project root: `target` when it ends in `.json`,
    /// `target/tsconfig.json` when it is a directory, else `target.json`.
    fn expected_file(&self, root: &Path, target: &str) -> String {
        if Path::new(target)
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            target.to_owned()
        } else if self.directories.contains(&normalize(&root.join(target))) {
            format!("{target}/tsconfig.json")
        } else {
            format!("{target}.json")
        }
    }

    /// Rewrites the tsconfig at `full` so that its `extends` names only `kept`, the bases that
    /// can be read; the rest of it stays as written. A tsconfig that cannot be rewritten stays as
    /// it is, and oxc reports what it cannot read.
    fn drop_bases(&mut self, full: &Path, kept: &[String]) {
        let Some(mut bytes) = self.contents.get(full).cloned() else {
            return;
        };
        if bytes.starts_with(BYTE_ORDER_MARK) {
            bytes[..BYTE_ORDER_MARK.len()].fill(b' ');
        }
        if json_strip_comments::strip_slice(&mut bytes).is_err() {
            return;
        }
        let Ok(serde_json::Value::Object(mut tsconfig)) = serde_json::from_slice(&bytes) else {
            return;
        };
        if kept.is_empty() {
            tsconfig.remove("extends");
        } else {
            tsconfig.insert(
                "extends".to_owned(),
                serde_json::Value::Array(
                    kept.iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
        if let Ok(rewritten) = serde_json::to_vec(&tsconfig) {
            self.contents.insert(full.to_path_buf(), rewritten);
        }
    }

    /// The base the relative `extends` target `target` (relative to the project root `root`)
    /// names, as oxc picks it: the file `target`, else `target/tsconfig.json` when `target` is a
    /// directory, else `target.json`. Its contents are loaded from `source`; `None` when none of
    /// those can be read.
    ///
    /// # Errors
    /// When the source cannot read a file it tracks.
    fn read_base(
        &mut self,
        source: &dyn Source,
        root: &Path,
        target: &str,
    ) -> Result<Option<String>> {
        let full = normalize(&root.join(target));
        let candidates = if self.files.contains(&full) {
            vec![target.to_owned()]
        } else if self.directories.contains(&full) {
            vec![format!("{target}/tsconfig.json")]
        } else {
            vec![
                target.to_owned(),
                format!("{target}/tsconfig.json"),
                format!("{target}.json"),
            ]
        };

        for candidate in candidates {
            let full = normalize(&root.join(&candidate));
            if self.contents.contains_key(&full) {
                return Ok(Some(candidate));
            }
            if let Some(bytes) = source.bytes(&candidate)? {
                self.add_contents(full, &bytes);
                return Ok(Some(candidate));
            }
        }

        Ok(None)
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

/// The options oxc resolves a `#` or package `extends` entry with: the conditions `node` and
/// `import`, the extension `.json`, and `tsconfig` as a directory's main file.
fn extends_options() -> ResolveOptions {
    ResolveOptions {
        condition_names: ["node", "import"].map(str::to_owned).to_vec(),
        extensions: vec![".json".to_owned()],
        main_files: vec!["tsconfig".to_owned()],
        symlinks: false,
        ..ResolveOptions::default()
    }
}

/// Every string in `value`, at any depth, into `strings_found`.
fn strings(value: &serde_json::Value, strings_found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => strings_found.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                strings(item, strings_found);
            }
        }
        serde_json::Value::Object(entries) => {
            for item in entries.values() {
                strings(item, strings_found);
            }
        }
        _ => {}
    }
}

/// `target` relative to the directory `directory` (both relative to the project root, slash
/// separated), with `.` removed and `..` applied; a `..` above the project root stays.
fn join_relative(directory: &str, target: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();

    for part in directory.split('/').chain(target.split('/')) {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    parts.join("/")
}

/// The repository path of `target` (relative to the project root `root`); `None` when it leaves
/// the repository.
fn repository_path(root: &Path, target: &str) -> Option<String> {
    normalize(&root.join(target))
        .strip_prefix(REPOSITORY)
        .ok()
        .and_then(relative_path)
}

/// Whether the resolver reads `path`: a `package.json`, `tsconfig*.json` or `jsconfig*.json`.
fn is_configuration(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);

    name == "package.json"
        || ((name.starts_with("tsconfig") || name.starts_with("jsconfig"))
            && has_extension(name, &["json"]))
}

/// `path` with `.` removed and `..` applied, without touching the file system.
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

impl FileSystem for SourceFileSystem {
    fn new() -> Self {
        Self::default()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize(path);

        self.contents.get(&path).cloned().ok_or_else(|| {
            if self.files.contains(&path) {
                io::Error::other(format!(
                    "{} is tracked but is not a configuration file the resolver reads",
                    path.display()
                ))
            } else {
                io::ErrorKind::NotFound.into()
            }
        })
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

    fn read_link(&self, path: &Path) -> std::result::Result<PathBuf, ResolveError> {
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

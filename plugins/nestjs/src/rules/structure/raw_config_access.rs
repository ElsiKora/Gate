//! `nestjs/raw-config-access`: feature code reads typed runtime configuration, never a raw
//! configuration service, and the example environment file lists exactly the keys the code reads.
//!
//! A `.ts` file below the source root outside the approved configuration boundary must not reach a
//! raw configuration export (`boundary`): a named export of a module `raw-imports` lists. The rule
//! follows names across the source files as the TypeScript checker does, without loading any
//! package: imports and re-exports, `export *` edges, namespaces, `const` aliases, destructuring,
//! `require`, `import(...)` types and `await import`. Nor may it read `process.env`
//! (`process-env`). The keys the getters of a raw service and `process.env` read are compared with
//! `example-file` by [`gate_typescript::env`].

mod reads;

use crate::shared::paths::{self, Form};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};
use gate_typescript::binding::ImportBindings;
use gate_typescript::env::{self, EnvRead, ExternalKey};
use gate_typescript::owners::export_name;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, ChainElement, ComputedMemberExpression, Declaration, ExportAllDeclaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportSpecifier, Expression,
    ModuleExportName, Statement, StaticMemberExpression, StringLiteral, TSImportTypeQualifier,
    TSModuleReference, TSNamespaceDeclarationBody, TSQualifiedName, TSTypeName,
    VariableDeclarationKind,
};
use gate_typescript::oxc_syntax::reference::ReferenceId;
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::{Result, SourceFile, TypeScript};

use crate::layout::PluginOptions;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct RawConfigAccess;

/// The named exports of one module that are raw configuration services.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct RawImport {
    /// The module specifier, matched exactly: a subpath of it does not match.
    pub module: String,
    /// The exported names of the module that are raw configuration services.
    pub names: Vec<String>,
}

/// The options of `nestjs/raw-config-access`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The raw configuration services; at least one.
    #[schemars(length(min = 1))]
    pub raw_imports: Vec<RawImport>,
    /// Files of the configuration boundary, by exact path.
    #[serde(default)]
    pub approved_files: Vec<String>,
    /// Path prefixes of the configuration boundary.
    #[serde(default)]
    pub approved_prefixes: Vec<String>,
    /// Path fragments of the configuration boundary, matched anywhere in a path.
    #[serde(default)]
    pub approved_segments: Vec<String>,
    /// The example environment file, relative to the project root.
    #[serde(default = "example_file")]
    pub example_file: String,
    /// The methods of a raw configuration service whose first argument is a key.
    #[serde(default = "getters")]
    pub getters: Vec<String>,
    /// Keys something other than the project's TypeScript reads (the runtime, a script, an image),
    /// each with what reads it; they count as read.
    #[serde(default)]
    pub external_keys: Vec<ExternalKey>,
}

fn example_file() -> String {
    env::EXAMPLE_FILE.to_owned()
}

fn getters() -> Vec<String> {
    vec!["get".to_owned(), "getOrThrow".to_owned()]
}

impl Options {
    /// Whether `path` is inside the approved configuration boundary.
    fn approves(&self, path: &str) -> bool {
        self.approved_files.iter().any(|file| file == path)
            || self
                .approved_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix.as_str()))
            || self
                .approved_segments
                .iter()
                .any(|segment| path.contains(segment.as_str()))
    }
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if self.raw_imports.is_empty() {
            return Err(gate_sdk::Error::config(
                "option `raw-imports` lists nothing; list at least one raw configuration module, or leave the rule off".to_owned(),
            ));
        }
        paths::check_all("approved-files", &self.approved_files, Form::File)?;
        paths::check_all(
            "example-file",
            std::slice::from_ref(&self.example_file),
            Form::File,
        )?;
        paths::check_all("approved-prefixes", &self.approved_prefixes, Form::Prefix)
    }
}

impl Rule<crate::Nest> for RawConfigAccess {
    type Options = Options;

    const NAME: &'static str = "raw-config-access";
    const TITLE: &'static str =
        "Only approved configuration boundaries reach a raw configuration service";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const ASPECTS: &'static [&'static str] = &[
        "boundary",
        "process-env",
        "undeclared",
        "unread",
        "dynamic",
        "example-missing",
        "example-syntax",
    ];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            run.options().check_paths()?;

            if run.options().raw_imports.is_empty() {
                return Ok(());
            }

            let source = shared.source();
            let typescript = shared.typescript()?;
            let mut sources = Vec::new();

            for path in tree::typescript_files(source, &layout.source_root, typescript.settings()) {
                if let Some(file) = project::parsed(typescript, path)? {
                    // Symbol and reference ids are set when the semantic model is built, and the graph
                    // reads them in files it reached through another file.
                    let _ = file.semantic();
                    sources.push((path, file));
                }
            }

            let options = run.options();
            let graph = Graph::new(typescript, &sources, &options.raw_imports);
            let mut reported = Vec::new();
            let mut process_env = Vec::new();
            let mut reads: Vec<EnvRead> = Vec::new();

            for (index, (path, _)) in sources.iter().enumerate() {
                let outside = !options.approves(path);
                if outside && graph.reads_raw(index)? {
                    reported.push(*path);
                }
                let found = graph.env_reads(index, &options.getters)?;
                if outside {
                    process_env.extend(found.process_env.into_iter().map(|read| (*path, read)));
                }
                reads.extend(found.reads);
            }
            let example_path = options.example_file.clone();
            let external = options.external_keys.clone();

            for path in reported {
                run.report()
                    .finding(
                        "boundary",
                        Location::file(path),
                        format!("{path} imports a raw config service outside an approved config boundary"),
                    )
                    .help("read the typed runtime configuration, or move the file into the configuration boundary")
                    .key("raw-import");
            }
            for (path, read) in process_env {
                run.report()
                    .finding(
                        "process-env",
                        Location::span(path, read.span),
                        format!("{path} reads `{}` outside an approved config boundary", read.text),
                    )
                    .target(format!("{path}:{}", read.span.line))
                    .help("read the typed runtime configuration, or move the read into the configuration boundary")
                    .key(read.text);
            }
            let example = run.tree().text(&example_path);
            env::check_example(run.report(), &example_path, example.as_deref(), &reads, &external);

            Ok(())
        })
    }
}

/// What a name denotes, as a symbol of the TypeScript checker.
#[derive(Clone, Copy, Debug)]
enum Entity<'a> {
    /// A source file as a module.
    Module(usize),
    /// A symbol a file declares.
    Local(usize, SymbolId),
    /// `export { a as b }`, with the module of `export { a as b } from "m"`.
    Specifier(
        usize,
        &'a ExportSpecifier<'a>,
        Option<&'a StringLiteral<'a>>,
    ),
    /// `export * as b from "m"`.
    Namespace(usize, &'a ExportAllDeclaration<'a>),
    /// `export default <expression>`.
    Default(usize, &'a ExportDefaultDeclaration<'a>),
}

/// The identity of an entity or expression on a resolution path, which a cycle never revisits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Key {
    Module(usize),
    Local(usize, SymbolId),
    Node(usize),
}

impl Entity<'_> {
    fn key(self) -> Key {
        match self {
            Self::Module(file) => Key::Module(file),
            Self::Local(file, symbol) => Key::Local(file, symbol),
            Self::Specifier(_, specifier, _) => Key::Node(std::ptr::from_ref(specifier) as usize),
            Self::Namespace(_, declaration) => Key::Node(std::ptr::from_ref(declaration) as usize),
            Self::Default(_, declaration) => Key::Node(std::ptr::from_ref(declaration) as usize),
        }
    }
}

/// What an entity's alias names, as the checker's immediate aliased symbol.
#[derive(Clone, Copy, Debug)]
enum Alias<'a> {
    /// The entity is no alias.
    None,
    /// The alias names a source entity.
    To(Entity<'a>),
    /// The alias names a package or nothing the source files declare.
    Unresolved,
}

impl<'a> Alias<'a> {
    fn of(target: Option<Entity<'a>>) -> Self {
        target.map_or(Self::Unresolved, Self::To)
    }
}

/// A module or namespace whose exports a member name selects.
#[derive(Clone, Copy, Debug)]
enum Container {
    Module(usize),
    Namespace(usize, SymbolId),
}

/// An expression or name the bindings of a file resolve.
#[derive(Clone, Copy)]
enum Probe<'a> {
    Expression(&'a Expression<'a>),
    Symbol(SymbolId),
    StaticMember(&'a StaticMemberExpression<'a>),
    ComputedMember(&'a ComputedMemberExpression<'a>),
    Qualified(&'a TSQualifiedName<'a>),
}

/// The exports of one module in the order the checker lists them, with an index by name.
#[derive(Debug, Default)]
struct Table<'a> {
    exports: Vec<(String, Entity<'a>)>,
    names: HashMap<String, usize>,
}

impl<'a> Table<'a> {
    /// Adds an export unless the name is taken: the first declaration of a name wins.
    fn add(&mut self, name: String, entity: Entity<'a>) {
        if !self.names.contains_key(&name) {
            self.names.insert(name.clone(), self.exports.len());
            self.exports.push((name, entity));
        }
    }

    fn get(&self, name: &str) -> Option<Entity<'a>> {
        self.names.get(name).map(|index| self.exports[*index].1)
    }
}

/// The source files with their exports, resolved modules and bindings, each read once per run.
struct Graph<'g, 'a> {
    typescript: &'g TypeScript<'a>,
    sources: &'g [(&'g str, Rc<SourceFile<'a>>)],
    index: HashMap<&'g str, usize>,
    raw: &'g [RawImport],
    targets: RefCell<HashMap<(usize, String), Option<usize>>>,
    tables: RefCell<HashMap<usize, Rc<Table<'a>>>>,
    /// Answers of `raw_symbol` reached with an empty path, which depend on nothing else.
    answers: RefCell<HashMap<(Key, Vec<String>), bool>>,
}

impl<'g, 'a> Graph<'g, 'a> {
    fn new(
        typescript: &'g TypeScript<'a>,
        sources: &'g [(&'g str, Rc<SourceFile<'a>>)],
        raw: &'g [RawImport],
    ) -> Self {
        Self {
            typescript,
            sources,
            index: sources
                .iter()
                .enumerate()
                .map(|(index, (path, _))| (*path, index))
                .collect(),
            raw,
            targets: RefCell::new(HashMap::new()),
            tables: RefCell::new(HashMap::new()),
            answers: RefCell::new(HashMap::new()),
        }
    }

    fn file(&self, index: usize) -> &'g SourceFile<'a> {
        &self.sources[index].1
    }

    /// Whether `name` of `module` is a raw configuration service.
    fn raw_name(&self, module: &str, name: &str) -> bool {
        self.raw
            .iter()
            .any(|raw| raw.module == module && raw.names.iter().any(|item| item == name))
    }

    fn raw_module(&self, module: &str) -> Option<usize> {
        self.raw.iter().position(|raw| raw.module == module)
    }

    /// The source file `specifier` names from `file`, as the compiler resolves it.
    fn target(&self, file: usize, specifier: &str) -> Result<Option<usize>> {
        let key = (file, specifier.to_owned());

        if let Some(target) = self.targets.borrow().get(&key) {
            return Ok(*target);
        }

        let target = self
            .typescript
            .resolve(self.sources[file].0, specifier)?
            .and_then(|path| self.index.get(path.as_str()).copied());
        self.targets.borrow_mut().insert(key, target);
        Ok(target)
    }

    /// Whether any name, export or import type of the file reaches a raw configuration service.
    fn reads_raw(&self, file: usize) -> Result<bool> {
        let source = self.file(file);
        let semantic = source.semantic();

        for node in semantic.nodes().iter() {
            let raw = match node.kind() {
                AstKind::ExportAllDeclaration(export) => {
                    self.raw_module(&export.source.value).is_some()
                        || match self.target(file, &export.source.value)? {
                            Some(module) => {
                                self.has_raw_export(Entity::Module(module), &mut Vec::new())?
                            }
                            None => false,
                        }
                }
                AstKind::ExportFromDeclaration(export) => {
                    let mut raw = false;

                    for specifier in &export.specifiers {
                        let entity = Entity::Specifier(file, specifier, Some(&export.source));

                        if self.raw_symbol(entity, &[], &mut Vec::new())? {
                            raw = true;
                            break;
                        }
                    }

                    raw
                }
                AstKind::TSImportType(import) => {
                    let mut names = Vec::new();
                    let mut qualifier = import.qualifier.as_ref();

                    while let Some(name) = qualifier {
                        match name {
                            TSImportTypeQualifier::Identifier(name) => {
                                names.insert(0, name.name.to_string());
                                qualifier = None;
                            }
                            TSImportTypeQualifier::QualifiedName(name) => {
                                names.insert(0, name.right.name.to_string());
                                qualifier = Some(&name.left);
                            }
                        }
                    }

                    if let Some(raw) = self.raw_module(&import.source.value) {
                        names
                            .first()
                            .is_some_and(|name| self.raw[raw].names.contains(name))
                    } else if let Some(module) = self.target(file, &import.source.value)? {
                        self.raw_symbol(Entity::Module(module), &names, &mut Vec::new())?
                    } else {
                        false
                    }
                }
                AstKind::IdentifierReference(identifier) => {
                    match identifier.reference_id.get().and_then(|reference| {
                        semantic.scoping().get_reference(reference).symbol_id()
                    }) {
                        Some(symbol) => self.raw_bound(file, symbol, &[], &mut Vec::new())?,
                        None => false,
                    }
                }
                AstKind::BindingIdentifier(identifier) => match identifier.symbol_id.get() {
                    Some(symbol) => self.raw_bound(file, symbol, &[], &mut Vec::new())?,
                    None => false,
                },
                AstKind::StaticMemberExpression(member) => {
                    self.raw_member(file, member, &[], &mut Vec::new())?
                }
                AstKind::ComputedMemberExpression(member) => {
                    self.raw_readers(file, Probe::ComputedMember(member), &[], &mut Vec::new())?
                }
                AstKind::TSQualifiedName(name) => {
                    let mut active = Vec::new();
                    // `typeof a.b` reads its name as an expression, through variables.
                    let mut id = node.id();
                    let values = loop {
                        let parent = semantic.nodes().parent_id(id);

                        match semantic.nodes().kind(parent) {
                            AstKind::TSQualifiedName(_) if parent != id => id = parent,
                            AstKind::TSTypeQuery(_) => break true,
                            _ => break false,
                        }
                    };

                    match self.qualified_entity(file, &name.left, &name.right.name, values)? {
                        Some(entity) if !self.destructured(entity) => {
                            self.raw_symbol(entity, &[], &mut active)?
                        }
                        _ => self.raw_readers(file, Probe::Qualified(name), &[], &mut active)?,
                    }
                }
                _ => false,
            };

            if raw {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// A name bound to `symbol` of `file`: a destructured binding through the bindings of the
    /// file, any other through its declarations.
    fn raw_bound(
        &self,
        file: usize,
        symbol: SymbolId,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        let entity = Entity::Local(file, symbol);

        if self.destructured(entity) {
            self.raw_readers(file, Probe::Symbol(symbol), members, active)
        } else {
            self.raw_symbol(entity, members, active)
        }
    }

    /// `a.b`: the export it selects when `a` is a module or namespace, otherwise the bindings of
    /// the file.
    fn raw_member(
        &self,
        file: usize,
        member: &'a StaticMemberExpression<'a>,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        match self.member_entity(file, &member.object, &member.property.name, true)? {
            Some(entity) if !self.destructured(entity) => self.raw_symbol(entity, members, active),
            _ => self.raw_readers(file, Probe::StaticMember(member), members, active),
        }
    }

    /// Whether the probe, followed by `members`, denotes a raw service through the module its
    /// binding reaches: a raw package or a source module. A binding ends at an import, `require`
    /// or `await import` of the file, so the module is one the file loads.
    fn raw_readers(
        &self,
        file: usize,
        probe: Probe<'a>,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        let bindings = ImportBindings::any(self.file(file), true);
        let path = match probe {
            Probe::Expression(expression) => bindings.resolve(expression),
            Probe::Symbol(symbol) => bindings.resolve_symbol(symbol),
            Probe::StaticMember(member) => bindings.resolve_static_member(member),
            Probe::ComputedMember(member) => bindings.resolve_computed_member(member),
            Probe::Qualified(name) => bindings.resolve_type_name(&name.left).map(|mut path| {
                path.push(name.right.name.to_string());
                path
            }),
        };
        let (Some(mut path), Some(module)) = (path, bindings.reached()) else {
            return Ok(false);
        };
        path.extend(members.iter().cloned());

        if let Some(raw) = self.raw_module(&module) {
            return Ok(path
                .first()
                .is_some_and(|name| self.raw[raw].names.contains(name)));
        }

        match self.target(file, &module)? {
            Some(target) => self.raw_symbol(Entity::Module(target), &path, active),
            None => Ok(false),
        }
    }

    /// Whether the entity, followed by `members`, is a raw service; `active` holds the entities and
    /// expressions on the current path.
    fn raw_symbol(
        &self,
        entity: Entity<'a>,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        let key = entity.key();

        if active.contains(&key) {
            return Ok(false);
        }

        if !active.is_empty() {
            active.push(key);
            let raw = self.raw_symbol_step(entity, members, active);
            active.pop();
            return raw;
        }

        let answer = (key, members.to_vec());

        if let Some(raw) = self.answers.borrow().get(&answer) {
            return Ok(*raw);
        }

        active.push(key);
        let raw = self.raw_symbol_step(entity, members, active)?;
        active.pop();
        self.answers.borrow_mut().insert(answer, raw);
        Ok(raw)
    }

    fn raw_symbol_step(
        &self,
        entity: Entity<'a>,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        match entity {
            Entity::Module(module) => self.raw_module_member(module, members, active),
            Entity::Local(file, symbol) => self.raw_local(file, symbol, members, active),
            // The member of the module first, then the aliased export, which the module path may
            // have cut off at a cycle through the module itself.
            Entity::Specifier(file, specifier, Some(source)) => {
                let imported = export_name(&specifier.local);

                if self.raw_name(&source.value, &imported) {
                    return Ok(true);
                }

                if let Some(module) = self.target(file, &source.value)?
                    && self.raw_symbol(
                        Entity::Module(module),
                        &prepend(imported, members),
                        active,
                    )?
                {
                    return Ok(true);
                }

                match self.immediate(entity)? {
                    Alias::To(target) => self.raw_symbol(target, members, active),
                    _ => Ok(false),
                }
            }
            Entity::Specifier(..) | Entity::Default(..) => match self.immediate(entity)? {
                Alias::To(target) => self.raw_symbol(target, members, active),
                _ => Ok(false),
            },
            Entity::Namespace(file, export) => {
                if members
                    .first()
                    .is_some_and(|member| self.raw_name(&export.source.value, member))
                {
                    return Ok(true);
                }

                match self.target(file, &export.source.value)? {
                    Some(module) => self.raw_symbol(Entity::Module(module), members, active),
                    None => Ok(false),
                }
            }
        }
    }

    /// The first member of a module: its export of that name, or else a member of a module it
    /// re-exports with `export *`, which the checker does not list for a package.
    fn raw_module_member(
        &self,
        module: usize,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        let Some((first, rest)) = members.split_first() else {
            return Ok(false);
        };

        if let Some(entity) = self.export(module, first)? {
            return self.raw_symbol(entity, rest, active);
        }

        for statement in &self.file(module).program().body {
            if let Statement::ExportAllDeclaration(export) = statement
                && export.exported.is_none()
            {
                if self.raw_name(&export.source.value, first) {
                    return Ok(true);
                }

                if let Some(target) = self.target(module, &export.source.value)?
                    && self.raw_symbol(Entity::Module(target), members, active)?
                {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn raw_local(
        &self,
        file: usize,
        symbol: SymbolId,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        let semantic = self.file(file).semantic();
        let nodes = semantic.nodes();

        for declaration in semantic.scoping().symbol_declarations(symbol) {
            match nodes.kind(declaration) {
                AstKind::ImportSpecifier(specifier) => {
                    let AstKind::ImportDeclaration(import) = nodes.parent_kind(declaration) else {
                        continue;
                    };
                    let imported = export_name(&specifier.imported);

                    if self.raw_name(&import.source.value, &imported) {
                        return Ok(true);
                    }

                    // The member of the module; the aliased export follows below.
                    if let Some(module) = self.target(file, &import.source.value)?
                        && self.raw_symbol(
                            Entity::Module(module),
                            &prepend(imported, members),
                            active,
                        )?
                    {
                        return Ok(true);
                    }
                }
                AstKind::ImportNamespaceSpecifier(_) => {
                    let AstKind::ImportDeclaration(import) = nodes.parent_kind(declaration) else {
                        continue;
                    };

                    if members
                        .first()
                        .is_some_and(|member| self.raw_name(&import.source.value, member))
                    {
                        return Ok(true);
                    }
                }
                AstKind::VariableDeclarator(declarator) => {
                    let constant = matches!(
                        nodes.parent_kind(declaration),
                        AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                    );

                    if constant
                        && matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol))
                        && let Some(init) = &declarator.init
                        && self.raw_expression(file, init, members, active)?
                    {
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }

        let entity = Entity::Local(file, symbol);

        match self.immediate(entity)? {
            Alias::To(target) => return self.raw_symbol(target, members, active),
            Alias::Unresolved => return Ok(false),
            Alias::None => {}
        }

        if let Some((first, rest)) = members.split_first()
            && let Some(member) = self.namespace_export(file, symbol, first)
        {
            return self.raw_symbol(member, rest, active);
        }

        Ok(false)
    }

    /// An expression a `const` binds: a name through its symbol, `a.b` through the export it
    /// selects, and any other loading form through the bindings of the file.
    fn raw_expression(
        &self,
        file: usize,
        expression: &'a Expression<'a>,
        members: &[String],
        active: &mut Vec<Key>,
    ) -> Result<bool> {
        let key = Key::Node(std::ptr::from_ref(expression) as usize);

        if active.contains(&key) {
            return Ok(false);
        }

        active.push(key);
        let raw = match expression {
            Expression::Identifier(identifier) => {
                match self.reference(file, identifier.reference_id.get()) {
                    Some(symbol) => self.raw_bound(file, symbol, members, active),
                    None => Ok(false),
                }
            }
            Expression::StaticMemberExpression(member) => {
                self.raw_member(file, member, members, active)
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => {
                    self.raw_member(file, member, members, active)
                }
                ChainElement::PrivateFieldExpression(_) => Ok(false),
                _ => self.raw_readers(file, Probe::Expression(expression), members, active),
            },
            Expression::ComputedMemberExpression(_)
            | Expression::CallExpression(_)
            | Expression::AwaitExpression(_)
            | Expression::TSAsExpression(_)
            | Expression::TSSatisfiesExpression(_)
            | Expression::ParenthesizedExpression(_)
            | Expression::TSNonNullExpression(_)
            | Expression::TSTypeAssertion(_) => {
                self.raw_readers(file, Probe::Expression(expression), members, active)
            }
            _ => Ok(false),
        };
        active.pop();
        raw
    }

    /// Whether a module that `export * from` names exposes a raw service: an export that is or
    /// leads to one, or a raw name it re-exports from a package.
    fn has_raw_export(&self, entity: Entity<'a>, active: &mut Vec<Key>) -> Result<bool> {
        let key = entity.key();

        if active.contains(&key) {
            return Ok(false);
        }

        if self.raw_symbol(entity, &[], &mut Vec::new())? {
            return Ok(true);
        }

        active.push(key);
        let raw = self.has_raw_export_step(entity, active);
        active.pop();
        raw
    }

    fn has_raw_export_step(&self, entity: Entity<'a>, active: &mut Vec<Key>) -> Result<bool> {
        let alias = self.immediate(entity)?;

        if !matches!(alias, Alias::None) {
            if let Entity::Namespace(_, export) = entity
                && self.raw_module(&export.source.value).is_some()
            {
                return Ok(true);
            }

            return match alias {
                Alias::To(target) => self.has_raw_export(target, active),
                _ => Ok(false),
            };
        }

        let exports: Vec<Entity<'a>> = match entity {
            Entity::Module(module) => self
                .table(module)?
                .exports
                .iter()
                .map(|(_, export)| *export)
                .collect(),
            Entity::Local(file, symbol) if self.is_namespace(file, symbol) => self
                .namespace_exports(file, symbol)
                .exports
                .iter()
                .map(|(_, export)| *export)
                .collect(),
            _ => return Ok(false),
        };

        for export in exports {
            if self.has_raw_export(export, active)? {
                return Ok(true);
            }
        }

        for raw in self.raw {
            for name in &raw.names {
                if self.raw_symbol(entity, std::slice::from_ref(name), &mut Vec::new())? {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// The entity an alias names, as the checker's immediate aliased symbol.
    fn immediate(&self, entity: Entity<'a>) -> Result<Alias<'a>> {
        match entity {
            Entity::Module(_) => Ok(Alias::None),
            Entity::Local(file, symbol) => {
                let semantic = self.file(file).semantic();
                let nodes = semantic.nodes();

                for declaration in semantic.scoping().symbol_declarations(symbol) {
                    let module = |id| match nodes.parent_kind(id) {
                        AstKind::ImportDeclaration(import) => Some(import.source.value.as_str()),
                        _ => None,
                    };

                    return Ok(Alias::of(match nodes.kind(declaration) {
                        AstKind::ImportSpecifier(specifier) => match module(declaration) {
                            Some(module) => {
                                self.module_export(file, module, &export_name(&specifier.imported))?
                            }
                            None => None,
                        },
                        AstKind::ImportDefaultSpecifier(_) => match module(declaration) {
                            Some(module) => self.module_export(file, module, "default")?,
                            None => None,
                        },
                        AstKind::ImportNamespaceSpecifier(_) => match module(declaration) {
                            Some(module) => self.target(file, module)?.map(Entity::Module),
                            None => None,
                        },
                        AstKind::TSImportEqualsDeclaration(import) => {
                            match &import.module_reference {
                                TSModuleReference::ExternalModuleReference(reference) => self
                                    .target(file, &reference.expression.value)?
                                    .map(Entity::Module),
                                TSModuleReference::IdentifierReference(reference) => self
                                    .reference(file, reference.reference_id.get())
                                    .map(|symbol| Entity::Local(file, symbol)),
                                TSModuleReference::QualifiedName(name) => self.qualified_entity(
                                    file,
                                    &name.left,
                                    &name.right.name,
                                    false,
                                )?,
                            }
                        }
                        _ => continue,
                    }));
                }

                Ok(Alias::None)
            }
            Entity::Specifier(file, specifier, Some(source)) => Ok(Alias::of(self.module_export(
                file,
                &source.value,
                &export_name(&specifier.local),
            )?)),
            Entity::Specifier(file, specifier, None) => Ok(Alias::of(match &specifier.local {
                ModuleExportName::IdentifierReference(reference) => self
                    .reference(file, reference.reference_id.get())
                    .map(|symbol| Entity::Local(file, symbol)),
                _ => None,
            })),
            Entity::Namespace(file, export) => Ok(Alias::of(
                self.target(file, &export.source.value)?.map(Entity::Module),
            )),
            // `export default` of a name or a dotted name is an alias; any other expression is not.
            Entity::Default(file, export) => match &export.declaration {
                ExportDefaultDeclarationKind::Identifier(identifier) => Ok(Alias::of(
                    self.reference(file, identifier.reference_id.get())
                        .map(|symbol| Entity::Local(file, symbol)),
                )),
                // The checker typed a dotted name that names no namespace, through variables.
                ExportDefaultDeclarationKind::StaticMemberExpression(member)
                    if entity_name(&member.object) =>
                {
                    Ok(Alias::of(self.member_entity(
                        file,
                        &member.object,
                        &member.property.name,
                        true,
                    )?))
                }
                _ => Ok(Alias::None),
            },
        }
    }

    /// The export `name` of the module `specifier` names from `file`.
    fn module_export(
        &self,
        file: usize,
        specifier: &str,
        name: &str,
    ) -> Result<Option<Entity<'a>>> {
        match self.target(file, specifier)? {
            Some(module) => self.export(module, name),
            None => Ok(None),
        }
    }

    fn export(&self, module: usize, name: &str) -> Result<Option<Entity<'a>>> {
        Ok(self.table(module)?.get(name))
    }

    /// The exports of a module as the checker lists them: its own, then those its `export *`
    /// edges add, without `default`, each name once and the first declaration winning.
    fn table(&self, module: usize) -> Result<Rc<Table<'a>>> {
        if let Some(table) = self.tables.borrow().get(&module) {
            return Ok(Rc::clone(table));
        }

        let table = Rc::new(
            self.exports_of(module, &mut Vec::new())?
                .unwrap_or_default(),
        );
        self.tables.borrow_mut().insert(module, Rc::clone(&table));
        Ok(table)
    }

    /// One visit of the checker's export walk: a module already visited in this walk adds nothing.
    fn exports_of(&self, module: usize, visited: &mut Vec<usize>) -> Result<Option<Table<'a>>> {
        if visited.contains(&module) {
            return Ok(None);
        }

        visited.push(module);
        let statements = &self.file(module).program().body;
        let mut table = Self::declared_exports(module, statements);
        let mut nested = Table::default();

        for statement in statements {
            if let Statement::ExportAllDeclaration(export) = statement
                && export.exported.is_none()
                && let Some(target) = self.target(module, &export.source.value)?
                && let Some(exports) = self.exports_of(target, visited)?
            {
                for (name, entity) in exports.exports {
                    if name != "default" {
                        nested.add(name, entity);
                    }
                }
            }
        }

        for (name, entity) in nested.exports {
            table.add(name, entity);
        }

        Ok(Some(table))
    }

    /// The exports the statements of a module or namespace block declare themselves.
    fn declared_exports(file: usize, statements: &'a [Statement<'a>]) -> Table<'a> {
        let mut table = Table::default();

        for statement in statements {
            match statement {
                Statement::ExportDeclaration(export) => {
                    for (name, symbol) in declaration_bindings(&export.declaration) {
                        table.add(name, Entity::Local(file, symbol));
                    }
                }
                Statement::ExportNamedDeclaration(export) => {
                    for specifier in &export.specifiers {
                        table.add(
                            export_name(&specifier.exported),
                            Entity::Specifier(file, specifier, None),
                        );
                    }
                }
                Statement::ExportFromDeclaration(export) => {
                    for specifier in &export.specifiers {
                        table.add(
                            export_name(&specifier.exported),
                            Entity::Specifier(file, specifier, Some(&export.source)),
                        );
                    }
                }
                Statement::ExportAllDeclaration(export) => {
                    if let Some(exported) = &export.exported {
                        table.add(export_name(exported), Entity::Namespace(file, export));
                    }
                }
                Statement::ExportDefaultDeclaration(export) => {
                    let symbol = match &export.declaration {
                        ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                            function.id.as_ref().and_then(|id| id.symbol_id.get())
                        }
                        ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                            class.id.as_ref().and_then(|id| id.symbol_id.get())
                        }
                        ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                            interface.id.symbol_id.get()
                        }
                        _ => None,
                    };
                    table.add(
                        "default".to_owned(),
                        symbol.map_or(Entity::Default(file, export), |symbol| {
                            Entity::Local(file, symbol)
                        }),
                    );
                }
                _ => {}
            }
        }

        table
    }

    fn is_namespace(&self, file: usize, symbol: SymbolId) -> bool {
        let semantic = self.file(file).semantic();

        semantic
            .scoping()
            .symbol_declarations(symbol)
            .any(|declaration| {
                matches!(
                    semantic.nodes().kind(declaration),
                    AstKind::TSNamespaceDeclaration(_)
                )
            })
    }

    /// The exports of every block of a namespace, a dotted inner namespace included.
    fn namespace_exports(&self, file: usize, symbol: SymbolId) -> Table<'a> {
        let semantic = self.file(file).semantic();
        let mut table = Table::default();

        for declaration in semantic.scoping().symbol_declarations(symbol) {
            if let AstKind::TSNamespaceDeclaration(namespace) = semantic.nodes().kind(declaration) {
                match &namespace.body {
                    TSNamespaceDeclarationBody::TSModuleBlock(block) => {
                        for (name, entity) in Self::declared_exports(file, &block.body).exports {
                            table.add(name, entity);
                        }
                    }
                    TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
                        if let Some(inner_symbol) = inner.id.symbol_id.get() {
                            table.add(inner.id.name.to_string(), Entity::Local(file, inner_symbol));
                        }
                    }
                }
            }
        }

        table
    }

    fn namespace_export(&self, file: usize, symbol: SymbolId, name: &str) -> Option<Entity<'a>> {
        if !self.is_namespace(file, symbol) {
            return None;
        }

        self.namespace_exports(file, symbol).get(name)
    }

    /// The export `name` selects from what `object` denotes: a module or namespace, followed
    /// through aliases and, where `values` holds, through variables as the checker types them.
    fn member_entity(
        &self,
        file: usize,
        object: &'a Expression<'a>,
        name: &str,
        values: bool,
    ) -> Result<Option<Entity<'a>>> {
        match self.expression_container(file, object, values, &mut Vec::new())? {
            Some(container) => self.container_member(container, name),
            None => Ok(None),
        }
    }

    /// The export `name` selects from the namespace or module a type name denotes, read through
    /// variables where `values` holds.
    fn qualified_entity(
        &self,
        file: usize,
        left: &'a TSTypeName<'a>,
        name: &str,
        values: bool,
    ) -> Result<Option<Entity<'a>>> {
        match self.type_container(file, left, values, &mut Vec::new())? {
            Some(container) => self.container_member(container, name),
            None => Ok(None),
        }
    }

    fn container_member(&self, container: Container, name: &str) -> Result<Option<Entity<'a>>> {
        match container {
            Container::Module(module) => self.export(module, name),
            Container::Namespace(file, symbol) => Ok(self.namespace_export(file, symbol, name)),
        }
    }

    fn expression_container(
        &self,
        file: usize,
        expression: &'a Expression<'a>,
        values: bool,
        seen: &mut Vec<Key>,
    ) -> Result<Option<Container>> {
        match expression {
            Expression::ParenthesizedExpression(inner) => {
                self.expression_container(file, &inner.expression, values, seen)
            }
            Expression::TSNonNullExpression(inner) => {
                self.expression_container(file, &inner.expression, values, seen)
            }
            Expression::TSSatisfiesExpression(inner) => {
                self.expression_container(file, &inner.expression, values, seen)
            }
            Expression::Identifier(identifier) => {
                match self.reference(file, identifier.reference_id.get()) {
                    Some(symbol) => self.container(Entity::Local(file, symbol), values, seen),
                    None => Ok(None),
                }
            }
            Expression::StaticMemberExpression(member) => {
                self.member_container(file, member, values, seen)
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => {
                    self.member_container(file, member, values, seen)
                }
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    fn member_container(
        &self,
        file: usize,
        member: &'a StaticMemberExpression<'a>,
        values: bool,
        seen: &mut Vec<Key>,
    ) -> Result<Option<Container>> {
        let Some(container) = self.expression_container(file, &member.object, values, seen)? else {
            return Ok(None);
        };

        match self.container_member(container, &member.property.name)? {
            Some(entity) => self.container(entity, values, seen),
            None => Ok(None),
        }
    }

    fn type_container(
        &self,
        file: usize,
        name: &'a TSTypeName<'a>,
        values: bool,
        seen: &mut Vec<Key>,
    ) -> Result<Option<Container>> {
        match name {
            TSTypeName::IdentifierReference(identifier) => {
                match self.reference(file, identifier.reference_id.get()) {
                    Some(symbol) => self.container(Entity::Local(file, symbol), values, seen),
                    None => Ok(None),
                }
            }
            TSTypeName::QualifiedName(name) => {
                let Some(container) = self.type_container(file, &name.left, values, seen)? else {
                    return Ok(None);
                };

                match self.container_member(container, &name.right.name)? {
                    Some(entity) => self.container(entity, values, seen),
                    None => Ok(None),
                }
            }
            TSTypeName::ThisExpression(_) => Ok(None),
        }
    }

    /// The module or namespace an entity denotes, through aliases and, where `values` holds,
    /// through variables initialized without a type annotation.
    fn container(
        &self,
        entity: Entity<'a>,
        values: bool,
        seen: &mut Vec<Key>,
    ) -> Result<Option<Container>> {
        let key = entity.key();

        if seen.contains(&key) {
            return Ok(None);
        }

        seen.push(key);

        match entity {
            Entity::Module(module) => Ok(Some(Container::Module(module))),
            Entity::Local(file, symbol) if self.is_namespace(file, symbol) => {
                Ok(Some(Container::Namespace(file, symbol)))
            }
            Entity::Local(file, symbol) => {
                match self.immediate(entity)? {
                    Alias::To(target) => return self.container(target, values, seen),
                    Alias::Unresolved => return Ok(None),
                    Alias::None => {}
                }

                if !values {
                    return Ok(None);
                }

                let semantic = self.file(file).semantic();

                for declaration in semantic.scoping().symbol_declarations(symbol) {
                    if let AstKind::VariableDeclarator(declarator) =
                        semantic.nodes().kind(declaration)
                        && declarator.type_annotation.is_none()
                        && matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol))
                        && let Some(init) = &declarator.init
                    {
                        return self.expression_container(file, init, values, seen);
                    }
                }

                Ok(None)
            }
            _ => match self.immediate(entity)? {
                Alias::To(target) => self.container(target, values, seen),
                _ => Ok(None),
            },
        }
    }

    /// Whether the entity is a name a destructuring pattern binds.
    fn destructured(&self, entity: Entity<'a>) -> bool {
        let Entity::Local(file, symbol) = entity else {
            return false;
        };
        let semantic = self.file(file).semantic();

        semantic.scoping().symbol_declarations(symbol).any(|declaration| {
            matches!(
                semantic.nodes().kind(declaration),
                AstKind::VariableDeclarator(declarator)
                    if !matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol))
            )
        })
    }

    fn reference(&self, file: usize, reference: Option<ReferenceId>) -> Option<SymbolId> {
        let reference = reference?;

        self.file(file)
            .semantic()
            .scoping()
            .get_reference(reference)
            .symbol_id()
    }
}

/// `first` followed by `rest`.
fn prepend(first: String, rest: &[String]) -> Vec<String> {
    let mut path = Vec::with_capacity(rest.len() + 1);
    path.push(first);
    path.extend(rest.iter().cloned());
    path
}

/// Whether `expression` is a name or a dotted name, which `export default` makes an alias.
fn entity_name(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Identifier(_) => true,
        Expression::StaticMemberExpression(member) => entity_name(&member.object),
        _ => false,
    }
}

/// The names and symbols an exported declaration binds.
fn declaration_bindings(declaration: &Declaration<'_>) -> Vec<(String, SymbolId)> {
    let identifiers = match declaration {
        Declaration::VariableDeclaration(variables) => variables
            .declarations
            .iter()
            .flat_map(|declarator| declarator.id.get_binding_identifiers())
            .collect(),
        Declaration::FunctionDeclaration(function) => function.id.iter().collect(),
        Declaration::ClassDeclaration(class) => class.id.iter().collect(),
        Declaration::TSTypeAliasDeclaration(alias) => vec![&alias.id],
        Declaration::TSInterfaceDeclaration(interface) => vec![&interface.id],
        Declaration::TSEnumDeclaration(enumeration) => vec![&enumeration.id],
        Declaration::TSNamespaceDeclaration(namespace) => vec![&namespace.id],
        Declaration::TSImportEqualsDeclaration(import) => vec![&import.id],
        Declaration::TSExternalModuleDeclaration(_) | Declaration::TSGlobalDeclaration(_) => {
            Vec::new()
        }
    };

    identifiers
        .into_iter()
        .filter_map(|identifier| Some((identifier.name.to_string(), identifier.symbol_id.get()?)))
        .collect()
}

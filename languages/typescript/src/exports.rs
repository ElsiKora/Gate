//! What each module exports, and the file that declares each name: the export map of a barrel,
//! followed through re-exports.
//!
//! An [`ExportMap`] reads a tracked module once, through the [`TypeScript`] view, and resolves its
//! re-exports with the resolver it was made with, so a rule decides which specifiers name which
//! files (the tsconfig `paths`, or the aliases of a layout). A module exports:
//!
//! - the names it declares (`export function`, `class`, `const`, `let`, `var`, `interface`,
//!   `type`, `enum`, `namespace`, `import x = require`), declared by the module itself;
//! - the names of `export { a as b }`: declared by the module itself when `a` is a local binding or
//!   a namespace import, otherwise followed to the module `a` is imported from;
//! - the names of `export { a as b } from "x"`, followed to `x`;
//! - `ns` of `export * as ns from "x"`, declared by the module itself;
//! - every name but `default` of the modules `export * from` reaches;
//! - `default`, for `export default`, which has no declaring file.
//!
//! A module is `open` when its names cannot all be known: an `export =`, an `export *` whose module
//! does not resolve, a file that does not parse, or such a module that an `export *` reaches.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use oxc_ast::ast::{Declaration, ImportDeclarationSpecifier, Statement};

use super::owners::export_name;
use super::{Result, SourceFile, TypeScript};

/// The resolver of an [`ExportMap`]: the tracked file a specifier names from an importer, or
/// `None` when it names none.
pub type Resolve<'c> = dyn Fn(&str, &str) -> Result<Option<String>> + 'c;

/// What one module exports by itself.
#[derive(Debug, Default)]
pub struct Own {
    /// The names the module declares itself.
    pub declared: BTreeSet<String>,
    /// The names it passes on by name: exported name, the file it comes from (`None` when the
    /// specifier names no tracked file), and the name there.
    pub named: Vec<(String, Option<String>, String)>,
    /// The tracked files its `export * from` reaches.
    pub stars: Vec<String>,
    /// The tracked files its `export … from` declarations resolve to, `export *` included.
    pub reexported: Vec<String>,
    /// Whether it has `export default`.
    pub default: bool,
    /// Whether its names cannot all be known.
    pub open: bool,
}

impl Own {
    /// The names the module exports by itself, without those of its stars.
    fn names(&self) -> impl Iterator<Item = &str> {
        self.declared
            .iter()
            .map(String::as_str)
            .chain(self.named.iter().map(|(name, _, _)| name.as_str()))
            .chain(self.default.then_some("default"))
    }
}

/// The names a module exports, followed through `export *`, each with its declaring file.
#[derive(Debug, Default)]
pub struct Exports {
    /// Each name and the file that declares it; `None` for a name with no declaring file: a
    /// default export, or a name passed on from a module that is not tracked or does not say.
    pub names: BTreeMap<String, Option<String>>,
    /// Whether one module on the way is open.
    pub open: bool,
}

/// The export map of a run: each module read once.
pub struct ExportMap<'c, 'a> {
    typescript: &'c TypeScript<'a>,
    resolve: Box<Resolve<'c>>,
    own: RefCell<HashMap<String, Rc<Own>>>,
    exports: RefCell<HashMap<String, Rc<Exports>>>,
}

impl std::fmt::Debug for ExportMap<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExportMap")
            .field("modules", &self.own.borrow().len())
            .finish_non_exhaustive()
    }
}

impl<'c, 'a> ExportMap<'c, 'a> {
    /// A map over `typescript` that resolves re-exports with `resolve`.
    pub fn new(
        typescript: &'c TypeScript<'a>,
        resolve: impl Fn(&str, &str) -> Result<Option<String>> + 'c,
    ) -> Self {
        Self {
            typescript,
            resolve: Box::new(resolve),
            own: RefCell::new(HashMap::new()),
            exports: RefCell::new(HashMap::new()),
        }
    }

    /// The syntax tree of a tracked file; `None` when it is not tracked or does not parse.
    fn parse(&self, path: &str) -> Result<Option<Rc<SourceFile<'a>>>> {
        Ok(self.typescript.parse(path)?.filter(|file| !file.fatal()))
    }

    /// What `path` exports by itself, read once.
    ///
    /// # Errors
    /// When the file cannot be read or a specifier cannot be resolved.
    pub fn own(&self, path: &str) -> Result<Rc<Own>> {
        if let Some(known) = self.own.borrow().get(path) {
            return Ok(known.clone());
        }
        let mut own = Own::default();
        match self.parse(path)? {
            None => own.open = true,
            Some(file) => self.read(path, &file, &mut own)?,
        }
        let own = Rc::new(own);
        self.own.borrow_mut().insert(path.to_owned(), own.clone());
        Ok(own)
    }

    /// The names `path` exports, without their declaring files. `export *` passes on every name
    /// but the default, so they are the module's own names and the names but the default of every
    /// module its stars reach. Each module is read once, so a cycle of `export *` ends and the
    /// answer does not depend on the order the files are read in.
    ///
    /// # Errors
    /// When a file cannot be read or a specifier cannot be resolved.
    pub fn names(&self, path: &str) -> Result<(BTreeSet<String>, bool)> {
        let mut names = BTreeSet::new();
        let mut open = false;
        let mut seen = HashSet::from([path.to_owned()]);
        let mut pending = vec![(path.to_owned(), true)];
        while let Some((module, root)) = pending.pop() {
            let own = self.own(&module)?;
            open |= own.open;
            names.extend(
                own.names()
                    .filter(|name| root || *name != "default")
                    .map(str::to_owned),
            );
            for star in &own.stars {
                if seen.insert(star.clone()) {
                    pending.push((star.clone(), false));
                }
            }
        }
        Ok((names, open))
    }

    /// The names `path` exports and the file that declares each, read once per module.
    ///
    /// # Errors
    /// When a file cannot be read or a specifier cannot be resolved.
    pub fn exports(&self, path: &str) -> Result<Rc<Exports>> {
        if let Some(known) = self.exports.borrow().get(path) {
            return Ok(known.clone());
        }
        let (names, open) = self.names(path)?;
        let mut exports = Exports {
            names: BTreeMap::new(),
            open,
        };
        for name in names {
            let declaring = self.declaring(path, &name, &mut HashSet::new())?;
            exports.names.insert(name, declaring);
        }
        let exports = Rc::new(exports);
        self.exports
            .borrow_mut()
            .insert(path.to_owned(), exports.clone());
        Ok(exports)
    }

    /// The file that declares the name `name` that `module` exports; `None` for a default export,
    /// a name the module does not export, or one that leaves the tracked files. A cycle of
    /// re-exports ends at the first module it reaches again.
    fn declaring(
        &self,
        module: &str,
        name: &str,
        seen: &mut HashSet<(String, String)>,
    ) -> Result<Option<String>> {
        if name == "default" || !seen.insert((module.to_owned(), name.to_owned())) {
            return Ok(None);
        }
        let own = self.own(module)?;
        if own.declared.contains(name) {
            return Ok(Some(module.to_owned()));
        }
        if let Some((_, from, imported)) =
            own.named.iter().find(|(exported, _, _)| exported == name)
        {
            return match from {
                Some(from) => self.declaring(from, imported, seen),
                None => Ok(None),
            };
        }
        for star in &own.stars {
            if let Some(found) = self.declaring(star, name, seen)? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    fn read(&self, path: &str, file: &SourceFile<'_>, own: &mut Own) -> Result<()> {
        // The local bindings imports make: local name, specifier, and the name imported, `None`
        // for a namespace.
        let mut imported: HashMap<String, (&str, Option<String>)> = HashMap::new();
        for statement in &file.program().body {
            if let Statement::ImportDeclaration(import) = statement {
                for specifier in import.specifiers.iter().flatten() {
                    let (local, name) = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(named) => {
                            (&named.local.name, Some(export_name(&named.imported)))
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                            (&default.local.name, Some("default".to_owned()))
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                            (&namespace.local.name, None)
                        }
                    };
                    imported.insert(local.to_string(), (import.source.value.as_str(), name));
                }
            }
        }

        for statement in &file.program().body {
            match statement {
                Statement::ExportDeclaration(export) => {
                    declaration_names(&export.declaration, &mut own.declared);
                }
                Statement::ExportNamedDeclaration(export) => {
                    for specifier in &export.specifiers {
                        let exported = export_name(&specifier.exported);
                        let local = export_name(&specifier.local);
                        match imported.get(&local) {
                            Some((source, Some(name))) => {
                                let from = (self.resolve)(path, source)?;
                                own.named.push((exported, from, name.clone()));
                            }
                            _ => {
                                own.declared.insert(exported);
                            }
                        }
                    }
                }
                Statement::ExportFromDeclaration(export) => {
                    let from = (self.resolve)(path, &export.source.value)?;
                    for specifier in &export.specifiers {
                        own.named.push((
                            export_name(&specifier.exported),
                            from.clone(),
                            export_name(&specifier.local),
                        ));
                    }
                    own.reexported.extend(from);
                }
                Statement::ExportDefaultDeclaration(_) => own.default = true,
                Statement::ExportAllDeclaration(export) => {
                    let from = (self.resolve)(path, &export.source.value)?;
                    match &export.exported {
                        Some(name) => {
                            own.declared.insert(export_name(name));
                        }
                        None => match &from {
                            Some(target) => own.stars.push(target.clone()),
                            None => own.open = true,
                        },
                    }
                    own.reexported.extend(from);
                }
                Statement::TSExportAssignment(_) => own.open = true,
                _ => {}
            }
        }
        Ok(())
    }
}

/// The names an exported declaration binds, destructured variables included.
fn declaration_names(declaration: &Declaration<'_>, names: &mut BTreeSet<String>) {
    match declaration {
        Declaration::VariableDeclaration(variable) => {
            for declarator in &variable.declarations {
                names.extend(
                    declarator
                        .id
                        .get_binding_identifiers()
                        .into_iter()
                        .map(|identifier| identifier.name.to_string()),
                );
            }
        }
        Declaration::FunctionDeclaration(function) => {
            names.extend(function.id.as_ref().map(|id| id.name.to_string()));
        }
        Declaration::ClassDeclaration(class) => {
            names.extend(class.id.as_ref().map(|id| id.name.to_string()));
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            names.insert(alias.id.name.to_string());
        }
        Declaration::TSInterfaceDeclaration(interface) => {
            names.insert(interface.id.name.to_string());
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            names.insert(enumeration.id.name.to_string());
        }
        Declaration::TSNamespaceDeclaration(namespace) => {
            names.insert(namespace.id.name.to_string());
        }
        Declaration::TSImportEqualsDeclaration(import) => {
            names.insert(import.id.name.to_string());
        }
        Declaration::TSExternalModuleDeclaration(_) | Declaration::TSGlobalDeclaration(_) => {}
    }
}

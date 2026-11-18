//! `typescript/barrel-imports`: a module outside a folder with a barrel imports from that folder
//! only through its barrel, and only names the barrel exports.
//!
//! A folder has a barrel when it holds a tracked file named `barrel-name`. For each static import
//! and re-export of a TypeScript file, the rule resolves the specifier the way the plugin resolves
//! modules (relative paths and the tsconfig `paths`), and finds the outermost folder with a barrel
//! that holds the imported file but not the importer. An import of any other file of that folder
//! is a deep path (`#deep-path`); an import of the barrel itself may name only what the barrel
//! exports (`#not-exported`). Files inside an owner folder (`owners`) import the folders inside that
//! owner directly, and importers that `exempt` matches are not checked. Type-only imports follow the
//! same rule.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Result, Rule, Run};
use gate_typescript::owners::export_name;
use gate_typescript::oxc_ast::ast::{Declaration, ImportDeclarationSpecifier, Statement};
use gate_typescript::{Allocator, ReadSource, Settings, SourceFile, TypeScript};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// The rule.
#[derive(Debug, Default)]
pub struct BarrelImports;

/// The options of `typescript/barrel-imports`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The file name of a folder's barrel: a name, not a path.
    #[schemars(length(min = 1), regex(pattern = r"^[^/\\]+$"))]
    pub barrel_name: String,
    /// Globs of the owner folders, relative to the project root: inside an owner, its files import
    /// its subfolders and each other directly.
    pub owners: Vec<String>,
    /// Globs of the importing files the rule does not check, relative to the project root: test
    /// roots, for example.
    pub exempt: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            barrel_name: "index.ts".to_owned(),
            owners: Vec::new(),
            exempt: Vec::new(),
        }
    }
}

impl Rule for BarrelImports {
    type Options = Options;

    const NAME: &'static str = "barrel-imports";
    const TITLE: &'static str = "A folder with a barrel is imported from outside only through its barrel, and only for names it exports";
    const ASPECTS: &'static [&'static str] = &["deep-path", "not-exported"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let options = run.options();
        let barrel_name = options.barrel_name.as_str();
        if barrel_name.is_empty()
            || barrel_name.contains(['/', '\\'])
            || barrel_name == "."
            || barrel_name == ".."
        {
            return Err(Error::new(format!(
                "barrel-name {barrel_name:?} is not a file name; write the name of a folder's \
                 barrel file, for example \"index.ts\""
            )));
        }
        let owners = globs("owners", &options.owners)?;
        let exempt = globs("exempt", &options.exempt)?;

        let tree = run.tree();
        let source = ReadSource::new(tree.paths(), move |path| tree.bytes(path));
        let settings = run.language::<Settings>("typescript")?.unwrap_or_default();
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, settings, &allocator);
        let checker = Checker {
            typescript: &typescript,
            files: typescript.files().iter().copied().collect(),
            barrel_name,
            owners: &owners,
            exports: RefCell::new(HashMap::new()),
        };

        let mut found = Vec::new();
        for path in typescript.files() {
            if typescript.settings().skips(path) || exempt.is_match(path) {
                continue;
            }
            let Some(file) = checker.parse(path)? else {
                continue;
            };
            for load in loads(&file) {
                checker.check(path, &load, &mut found)?;
            }
        }

        for finding in found {
            let help = match finding.aspect {
                "deep-path" => format!(
                    "import from the folder's barrel {} instead; a name the barrel does not \
                     export is internal to {}: export it from the barrel, or keep the import \
                     inside the folder",
                    finding.barrel, finding.folder
                ),
                _ => format!(
                    "export the name from the barrel {}, or keep the import inside {}",
                    finding.barrel, finding.folder
                ),
            };
            run.report()
                .finding(
                    finding.aspect,
                    Location::line(&finding.path, finding.line),
                    finding.message,
                )
                .target(finding.path.clone())
                .help(help);
        }
        Ok(())
    }
}

/// Globs whose `*` does not cross `/`, from option `key`.
fn globs(key: &str, patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| {
                    Error::new(format!(
                        "{key} glob {pattern:?} is not valid: {error}; write a glob relative to \
                         the project root, such as \"src/modules/*\""
                    ))
                })?,
        );
    }
    builder
        .build()
        .map_err(|error| Error::new(format!("{key} globs do not build: {error}")))
}

/// One module a file loads statically, and the names it takes from it.
struct Load {
    specifier: String,
    /// The names imported, `default` for a default import; `None` when the whole module is taken
    /// (`import * as`, `export *`) or nothing named is (`import "x"`).
    names: Option<Vec<String>>,
    /// The one-based line of the declaration.
    line: usize,
}

/// The static imports and re-exports of a file.
fn loads(file: &SourceFile<'_>) -> Vec<Load> {
    let line = |start: u32| usize::try_from(file.line(start)).unwrap_or(usize::MAX);
    let mut loads = Vec::new();
    for statement in &file.program().body {
        match statement {
            Statement::ImportDeclaration(import) => {
                let names = match &import.specifiers {
                    None => None,
                    Some(specifiers) if specifiers.is_empty() => None,
                    Some(specifiers) => specifiers
                        .iter()
                        .map(|specifier| match specifier {
                            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                                Some(export_name(&named.imported))
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {
                                Some("default".to_owned())
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => None,
                        })
                        .collect::<Option<Vec<_>>>(),
                };
                loads.push(Load {
                    specifier: import.source.value.to_string(),
                    names,
                    line: line(import.span.start),
                });
            }
            Statement::ExportFromDeclaration(export) => loads.push(Load {
                specifier: export.source.value.to_string(),
                names: Some(
                    export
                        .specifiers
                        .iter()
                        .map(|specifier| export_name(&specifier.local))
                        .collect(),
                ),
                line: line(export.span.start),
            }),
            Statement::ExportAllDeclaration(export) => loads.push(Load {
                specifier: export.source.value.to_string(),
                names: None,
                line: line(export.span.start),
            }),
            _ => {}
        }
    }
    loads
}

/// The names a module exports; `open` when they cannot all be known (an `export =`, a re-export
/// from a module that does not resolve to a tracked file, a destructured export).
#[derive(Clone, Debug, Default)]
struct Exports {
    names: BTreeSet<String>,
    open: bool,
}

/// One finding before it is reported.
struct Found {
    aspect: &'static str,
    path: String,
    line: usize,
    message: String,
    barrel: String,
    folder: String,
}

struct Checker<'c, 'a> {
    typescript: &'c TypeScript<'a>,
    files: HashSet<&'a str>,
    barrel_name: &'c str,
    owners: &'c GlobSet,
    /// The exports of each module asked, by path; `None` while it is being read (a cycle).
    exports: RefCell<HashMap<String, Option<Exports>>>,
}

impl<'a> Checker<'_, 'a> {
    /// The syntax tree of a tracked file; `None` when it is not tracked or does not parse, which
    /// `typescript/parse` reports.
    fn parse(&self, path: &str) -> Result<Option<std::rc::Rc<SourceFile<'a>>>> {
        let file = self
            .typescript
            .parse(path)
            .map_err(|error| Error::new(error.to_string()))?;
        Ok(file.filter(|file| !file.fatal()))
    }

    fn check(&self, path: &str, load: &Load, found: &mut Vec<Found>) -> Result<()> {
        let Some(target) = self
            .typescript
            .resolve(path, &load.specifier)
            .map_err(|error| Error::new(error.to_string()))?
        else {
            return Ok(());
        };
        let Some(folder) = self.barrel_folder(path, &target) else {
            return Ok(());
        };
        let barrel = join(&folder, self.barrel_name);
        let specifier = &load.specifier;

        if target != barrel {
            found.push(Found {
                aspect: "deep-path",
                path: path.to_owned(),
                line: load.line,
                message: format!(
                    "{path} imports {specifier} ({target}) past the barrel {barrel} of {folder}"
                ),
                barrel,
                folder,
            });
            return Ok(());
        }

        let Some(names) = &load.names else {
            return Ok(());
        };
        let exports = self.exports(&barrel)?;
        if exports.open {
            return Ok(());
        }
        let mut reported = BTreeSet::new();
        for name in names {
            if !exports.names.contains(name) && reported.insert(name) {
                found.push(Found {
                    aspect: "not-exported",
                    path: path.to_owned(),
                    line: load.line,
                    message: format!(
                        "{path} imports {name} from the barrel {barrel}, which does not export it"
                    ),
                    barrel: barrel.clone(),
                    folder: folder.clone(),
                });
            }
        }
        Ok(())
    }

    /// The outermost folder that holds `target` and a barrel but not `importer`, skipping the
    /// folders inside an owner folder that holds the importer.
    fn barrel_folder(&self, importer: &str, target: &str) -> Option<String> {
        let importer_owners: Vec<&str> = ancestors(parent(importer))
            .filter(|folder| self.owners.is_match(folder))
            .collect();
        ancestors(parent(target))
            .find(|folder| {
                !inside(folder, importer)
                    && self.files.contains(join(folder, self.barrel_name).as_str())
                    && !importer_owners.iter().any(|owner| inside(owner, folder))
            })
            .map(str::to_owned)
    }

    /// The names `path` exports, following `export * from` through tracked files.
    fn exports(&self, path: &str) -> Result<Exports> {
        if let Some(known) = self.exports.borrow().get(path) {
            // A module still being read is a cycle; what it adds is read by its first visit.
            return Ok(known.clone().unwrap_or_default());
        }
        self.exports.borrow_mut().insert(path.to_owned(), None);

        let mut exports = Exports::default();
        match self.parse(path)? {
            None => exports.open = true,
            Some(file) => {
                for statement in &file.program().body {
                    self.statement_exports(path, statement, &mut exports)?;
                }
            }
        }
        self.exports
            .borrow_mut()
            .insert(path.to_owned(), Some(exports.clone()));
        Ok(exports)
    }

    fn statement_exports(
        &self,
        path: &str,
        statement: &Statement<'_>,
        exports: &mut Exports,
    ) -> Result<()> {
        match statement {
            Statement::ExportDeclaration(export) => {
                declaration_names(&export.declaration, exports);
            }
            Statement::ExportNamedDeclaration(export) => {
                exports.names.extend(
                    export
                        .specifiers
                        .iter()
                        .map(|specifier| export_name(&specifier.exported)),
                );
            }
            Statement::ExportFromDeclaration(export) => {
                exports.names.extend(
                    export
                        .specifiers
                        .iter()
                        .map(|specifier| export_name(&specifier.exported)),
                );
            }
            Statement::ExportDefaultDeclaration(_) => {
                exports.names.insert("default".to_owned());
            }
            Statement::ExportAllDeclaration(export) => match &export.exported {
                Some(name) => {
                    exports.names.insert(export_name(name));
                }
                None => {
                    let target = self
                        .typescript
                        .resolve(path, &export.source.value)
                        .map_err(|error| Error::new(error.to_string()))?;
                    match target {
                        Some(target) => {
                            let inner = self.exports(&target)?;
                            exports.open |= inner.open;
                            // `export *` passes on every name but the default.
                            exports
                                .names
                                .extend(inner.names.into_iter().filter(|name| name != "default"));
                        }
                        None => exports.open = true,
                    }
                }
            },
            Statement::TSExportAssignment(_) => exports.open = true,
            _ => {}
        }
        Ok(())
    }
}

/// The names an exported declaration binds.
fn declaration_names(declaration: &Declaration<'_>, exports: &mut Exports) {
    let names = &mut exports.names;
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

/// The folder of a path; empty at the project root.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The folders from the outermost to `folder` itself, without the project root: `a`, `a/b`,
/// `a/b/c` for `a/b/c`.
fn ancestors(folder: &str) -> impl Iterator<Item = &str> {
    folder
        .match_indices('/')
        .map(|(index, _)| &folder[..index])
        .chain((!folder.is_empty()).then_some(folder))
}

/// Whether `path` lies inside `folder`.
fn inside(folder: &str, path: &str) -> bool {
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// `name` in `folder`.
fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

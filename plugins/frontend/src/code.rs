//! The project as the rules read it: its tracked paths outside the skipped folders, its TypeScript
//! sources, and what a parsed file imports, exports and declares.

use std::rc::Rc;

use gate_sdk::Run;
use gate_typescript::call::{Sources, TypeScriptCall, parsed, settings};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, IdentifierReference,
    ImportOrExportKind, Statement,
};
use gate_typescript::value::string_like;
use gate_typescript::{SourceFile, TypeScript};

use crate::options::Pack;
use crate::standard::{FileName, SOURCE_ROOT, name_of, within};

/// The project of one run: its pack, its tracked paths outside the skipped folders, and its
/// TypeScript view. `'s` is the borrow of the call's shared sources, `'o` what they hold.
pub struct Project<'s, 'o> {
    /// The pack of `[plugins.frontend] options`.
    pub pack: Pack,
    /// Every tracked path outside the skipped folders, in path order.
    pub paths: Vec<&'o str>,
    typescript: &'s TypeScript<'o>,
}

impl std::fmt::Debug for Project<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Project")
            .field("pack", &self.pack)
            .field("paths", &self.paths.len())
            .finish_non_exhaustive()
    }
}

impl<'s, 'o> Project<'s, 'o> {
    /// The project of `run`, from the sources its call shares.
    ///
    /// # Errors
    /// When the plugin's options have no pack, or the `[languages.typescript]` settings do not
    /// match.
    pub fn new<O>(
        run: &Run<'_, O, TypeScriptCall>,
        shared: &'s Sources<'o>,
    ) -> gate_sdk::Result<Self> {
        let pack = Pack::read(run)?;
        let settings = settings(run)?;
        let typescript = shared.typescript()?;
        let paths = shared
            .source()
            .paths()
            .filter(|path| !settings.skips(path))
            .collect();

        Ok(Self {
            pack,
            paths,
            typescript,
        })
    }

    /// The tracked paths below the source root.
    pub fn sources(&self) -> impl Iterator<Item = &'o str> + '_ {
        self.paths
            .iter()
            .copied()
            .filter(|path| within(path, SOURCE_ROOT))
    }

    /// The `.ts` and `.tsx` files below the source root, without declaration files.
    pub fn code(&self) -> impl Iterator<Item = &'o str> + '_ {
        self.sources().filter(|path| {
            let name = FileName::new(name_of(path));
            (name.extension == "ts" || name.extension == "tsx") && !name.dotted.contains(&"d")
        })
    }

    /// Whether `path` is tracked and outside the skipped folders.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.paths.binary_search(&path).is_ok()
    }

    /// The syntax tree of `path`; `None` when it cannot be parsed, which `typescript/parse` reports.
    ///
    /// # Errors
    /// When the file is tracked but cannot be read.
    pub fn parse(&self, path: &str) -> gate_sdk::Result<Option<Rc<SourceFile<'o>>>> {
        parsed(self.typescript, path)
    }

    /// The text of the tracked file `path`; `None` when it is not tracked.
    ///
    /// # Errors
    /// When the file cannot be read or is not UTF-8 text.
    pub fn text(&self, path: &str) -> gate_sdk::Result<Option<Rc<str>>> {
        Ok(self.typescript.source().text(path)?)
    }

    /// The tracked file `specifier` names from `importer`, resolved with the tsconfig `paths`.
    ///
    /// # Errors
    /// When the tsconfig cannot be read.
    pub fn resolve(&self, importer: &str, specifier: &str) -> gate_sdk::Result<Option<String>> {
        Ok(self.typescript.resolve(importer, specifier)?)
    }
}

/// The project path a relative or `@/` specifier names from `importer`, by its text alone: for files
/// the TypeScript resolver does not look up, such as stylesheets. `None` for any other specifier or
/// a path that leaves the project.
#[must_use]
pub fn joined(importer: &str, specifier: &str) -> Option<String> {
    let (base, rest) = if let Some(rest) = specifier.strip_prefix("@/") {
        (SOURCE_ROOT, rest)
    } else if specifier.starts_with("./") || specifier.starts_with("../") {
        (crate::standard::folder_of(importer), specifier)
    } else {
        return None;
    };
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// One module a file loads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// The specifier as written.
    pub specifier: String,
    /// The one-based line of the declaration or expression.
    pub line: u32,
    /// Whether the file loads it only as a type.
    pub type_only: bool,
}

/// Every module `file` loads, in source order: import declarations, re-exports, `import()`
/// expressions with a literal specifier and `import("...")` types.
#[must_use]
pub fn imports(file: &SourceFile<'_>) -> Vec<Import> {
    let mut found = Vec::new();

    for node in file.semantic().nodes().iter() {
        let (specifier, start, type_only) = match node.kind() {
            AstKind::ImportDeclaration(import) => (
                import.source.value.as_str().to_owned(),
                import.span.start,
                gate_typescript::modules::is_type_only_import(import),
            ),
            AstKind::ExportFromDeclaration(export) => (
                export.source.value.as_str().to_owned(),
                export.span.start,
                export.export_kind == ImportOrExportKind::Type,
            ),
            AstKind::ExportAllDeclaration(export) => (
                export.source.value.as_str().to_owned(),
                export.span.start,
                export.export_kind == ImportOrExportKind::Type,
            ),
            AstKind::ImportExpression(import) => match string_like(&import.source) {
                Some(specifier) => (specifier, import.span.start, false),
                None => continue,
            },
            AstKind::TSImportType(import) => (
                import.source.value.as_str().to_owned(),
                import.span.start,
                true,
            ),
            _ => continue,
        };

        found.push(Import {
            specifier,
            line: file.line(start),
            type_only,
        });
    }

    found.sort_by_key(|import| import.line);
    found
}

/// One name a file exports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Export {
    /// The exported name; `default` for the default export.
    pub name: String,
    /// The one-based line of the export.
    pub line: u32,
    /// Whether the name is a type only: an interface, a type alias or an `export type`.
    pub type_only: bool,
    /// Whether the export names a module (`export { a } from "./a"`).
    pub reexport: bool,
}

/// What a file exports: its names in source order, and whether an `export *` may add more.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Exports {
    /// Every exported name.
    pub names: Vec<Export>,
    /// Whether the file has an `export * from`, so its names are not all known here.
    pub star: bool,
}

impl Exports {
    /// The export named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Export> {
        self.names.iter().find(|export| export.name == name)
    }
}

/// The exports of `file`, from its top-level statements.
#[must_use]
pub fn exports(file: &SourceFile<'_>) -> Exports {
    let mut found = Exports::default();
    let mut add = |name: &str, start: u32, type_only: bool, reexport: bool| {
        found.names.push(Export {
            name: name.to_owned(),
            line: file.line(start),
            type_only,
            reexport,
        });
    };

    for statement in &file.program().body {
        match statement {
            Statement::ExportDeclaration(export) => {
                declared_names(&export.declaration, &mut |name, type_only| {
                    add(name, export.span.start, type_only, false);
                });
            }
            Statement::ExportNamedDeclaration(export) => {
                for specifier in &export.specifiers {
                    add(
                        specifier.exported.name().as_str(),
                        specifier.span.start,
                        export.export_kind == ImportOrExportKind::Type
                            || specifier.export_kind == ImportOrExportKind::Type,
                        false,
                    );
                }
            }
            Statement::ExportFromDeclaration(export) => {
                for specifier in &export.specifiers {
                    add(
                        specifier.exported.name().as_str(),
                        specifier.span.start,
                        export.export_kind == ImportOrExportKind::Type
                            || specifier.export_kind == ImportOrExportKind::Type,
                        true,
                    );
                }
            }
            Statement::ExportAllDeclaration(export) => {
                if let Some(name) = &export.exported {
                    add(name.name().as_str(), export.span.start, false, true);
                }
            }
            Statement::ExportDefaultDeclaration(export) => add(
                "default",
                export.span.start,
                matches!(
                    export.declaration,
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(_)
                ),
                false,
            ),
            _ => {}
        }
    }

    found.star = file.program().body.iter().any(|statement| {
        matches!(statement, Statement::ExportAllDeclaration(export) if export.exported.is_none())
    });
    found
}

/// The names a declaration binds, each with whether it is a type only.
fn declared_names(declaration: &Declaration<'_>, add: &mut dyn FnMut(&str, bool)) {
    match declaration {
        Declaration::VariableDeclaration(variables) => {
            for declarator in &variables.declarations {
                binding_names(&declarator.id, &mut |name| add(name, false));
            }
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                add(id.name.as_str(), false);
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                add(id.name.as_str(), false);
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => add(alias.id.name.as_str(), true),
        Declaration::TSInterfaceDeclaration(interface) => add(interface.id.name.as_str(), true),
        Declaration::TSEnumDeclaration(enumeration) => add(enumeration.id.name.as_str(), false),
        _ => {}
    }
}

/// The names a binding pattern binds.
fn binding_names(pattern: &BindingPattern<'_>, add: &mut dyn FnMut(&str)) {
    match pattern {
        BindingPattern::BindingIdentifier(identifier) => add(identifier.name.as_str()),
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                binding_names(&property.value, add);
            }
            if let Some(rest) = &object.rest {
                binding_names(&rest.argument, add);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                binding_names(element, add);
            }
            if let Some(rest) = &array.rest {
                binding_names(&rest.argument, add);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => binding_names(&assignment.left, add),
    }
}

/// Whether `file` starts with the directive `directive` (`"use client"`).
#[must_use]
pub fn has_directive(file: &SourceFile<'_>, directive: &str) -> bool {
    file.program()
        .directives
        .iter()
        .any(|found| found.directive.as_str() == directive)
}

/// Whether `identifier` names a global: no declaration of the file binds it.
#[must_use]
pub fn is_global(file: &SourceFile<'_>, identifier: &IdentifierReference<'_>) -> bool {
    identifier.reference_id.get().is_none_or(|reference| {
        file.semantic()
            .scoping()
            .get_reference(reference)
            .symbol_id()
            .is_none()
    })
}

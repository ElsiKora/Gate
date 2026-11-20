//! The project as the rules read it: its tracked paths outside the skipped folders, its TypeScript
//! sources, and what a parsed file imports, exports and declares.

use std::rc::Rc;

use gate_sdk::Run;
use gate_typescript::call::{Sources, TypeScriptCall, parsed, settings};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, BindingPattern, CallExpression, Declaration, ExportDefaultDeclarationKind,
    Expression, IdentifierReference, ImportOrExportKind, ModuleExportName, Statement,
    TSModuleReference,
};
use gate_typescript::value::string_like;
use gate_typescript::{SourceFile, TypeScript};

use crate::options::Pack;
use crate::standard::{
    FileName, Groups, NEXT_APP, NEXT_ROOT_FILES, Place, SOURCE_ROOT, name_of, place, within,
};

/// The project of one run: its pack, its tracked paths outside the skipped folders, and its
/// TypeScript view. `'s` is the borrow of the call's shared sources, `'o` what they hold.
pub struct Project<'s, 'o> {
    /// The pack of `[plugins.frontend] options`.
    pub pack: Pack,
    /// Every tracked path outside the skipped folders, in path order.
    pub paths: Vec<&'o str>,
    /// The slice groups among the paths.
    pub groups: Groups,
    /// The tsconfig `[languages.typescript]` names, relative to the project root.
    pub tsconfig: String,
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
        let paths: Vec<&'o str> = shared
            .source()
            .paths()
            .filter(|path| !settings.skips(path))
            .collect();
        let groups = Groups::new(pack, paths.iter().copied());

        Ok(Self {
            pack,
            paths,
            groups,
            tsconfig: settings.tsconfig,
            typescript,
        })
    }

    /// Where `path` sits in the standard tree; `None` when it is not below the source root.
    #[must_use]
    pub fn place<'p>(&self, path: &'p str) -> Option<Place<'p>> {
        place(self.pack, &self.groups, path)
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

    /// The `.ts` and `.tsx` files of the root `app/` of a Next.js app, its route files; none in the
    /// other packs.
    pub fn routes(&self) -> impl Iterator<Item = &'o str> + '_ {
        self.paths.iter().copied().filter(|path| {
            self.pack == Pack::Nextjs && within(path, NEXT_APP) && {
                let name = FileName::new(name_of(path));
                (name.extension == "ts" || name.extension == "tsx") && !name.dotted.contains(&"d")
            }
        })
    }

    /// The files Next.js looks up at the project root ([`NEXT_ROOT_FILES`]) that are tracked; none
    /// in the other packs.
    pub fn next_roots(&self) -> impl Iterator<Item = &'o str> + '_ {
        self.paths
            .iter()
            .copied()
            .filter(|path| self.pack == Pack::Nextjs && NEXT_ROOT_FILES.contains(path))
    }

    /// The code of [`Project::code`], the route files of [`Project::routes`] and the root files of
    /// [`Project::next_roots`].
    pub fn all_code(&self) -> impl Iterator<Item = &'o str> + '_ {
        self.code().chain(self.routes()).chain(self.next_roots())
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
/// expressions with a literal specifier, `import("...")` types, `import x = require("...")` and a
/// call of the global `require` with one literal argument.
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
            AstKind::TSImportEqualsDeclaration(import) => match &import.module_reference {
                TSModuleReference::ExternalModuleReference(reference) => (
                    reference.expression.value.as_str().to_owned(),
                    import.span.start,
                    import.import_kind == ImportOrExportKind::Type,
                ),
                _ => continue,
            },
            AstKind::CallExpression(call) => match required(file, call) {
                Some(specifier) => (specifier, call.span.start, false),
                None => continue,
            },
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

/// The module a call of the global `require` with one literal argument loads.
fn required(file: &SourceFile<'_>, call: &CallExpression<'_>) -> Option<String> {
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let [argument] = call.arguments.as_slice() else {
        return None;
    };
    if callee.name != "require" || !is_global(file, callee) {
        return None;
    }
    argument.as_expression().and_then(string_like)
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
    /// Whether the exported value is a function: a function or class declaration, or a `const`
    /// bound to a function or class expression, or to `memo(...)` or `forwardRef(...)` of one. A
    /// re-export from another module is not known to be one.
    pub function: bool,
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
    let mut add = |name: &str, start: u32, type_only: bool, reexport: bool, function: bool| {
        found.names.push(Export {
            name: name.to_owned(),
            line: file.line(start),
            type_only,
            reexport,
            function,
        });
    };

    for statement in &file.program().body {
        match statement {
            Statement::ExportDeclaration(export) => {
                declared_names(&export.declaration, &mut |name, type_only, function| {
                    add(name, export.span.start, type_only, false, function);
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
                        local_function(file, &specifier.local),
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
                        false,
                    );
                }
            }
            Statement::ExportAllDeclaration(export) => {
                if let Some(name) = &export.exported {
                    add(name.name().as_str(), export.span.start, false, true, false);
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
                matches!(
                    export.declaration,
                    ExportDefaultDeclarationKind::FunctionDeclaration(_)
                        | ExportDefaultDeclarationKind::ClassDeclaration(_)
                ),
            ),
            _ => {}
        }
    }

    found.star = file.program().body.iter().any(|statement| {
        matches!(statement, Statement::ExportAllDeclaration(export) if export.exported.is_none())
    });
    found
}

/// The names a declaration binds, each with whether it is a type only and whether its value is a
/// function ([`Export::function`]).
fn declared_names(declaration: &Declaration<'_>, add: &mut dyn FnMut(&str, bool, bool)) {
    match declaration {
        Declaration::VariableDeclaration(variables) => {
            for declarator in &variables.declarations {
                let function = matches!(declarator.id, BindingPattern::BindingIdentifier(_))
                    && declarator.init.as_ref().is_some_and(function_value);
                binding_names(&declarator.id, &mut |name| add(name, false, function));
            }
        }
        Declaration::FunctionDeclaration(function) => {
            if let Some(id) = &function.id {
                add(id.name.as_str(), false, true);
            }
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                add(id.name.as_str(), false, true);
            }
        }
        Declaration::TSTypeAliasDeclaration(alias) => add(alias.id.name.as_str(), true, false),
        Declaration::TSInterfaceDeclaration(interface) => {
            add(interface.id.name.as_str(), true, false);
        }
        Declaration::TSEnumDeclaration(enumeration) => {
            add(enumeration.id.name.as_str(), false, false);
        }
        _ => {}
    }
}

/// Whether `value` is a function: a function or class expression, or `memo(...)` or
/// `forwardRef(...)` (also as a member, `React.memo(...)`) of one.
fn function_value(value: &Expression<'_>) -> bool {
    match value {
        Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_)
        | Expression::ClassExpression(_) => true,
        Expression::CallExpression(call) => {
            let wrapper = match &call.callee {
                Expression::Identifier(callee) => callee.name.as_str(),
                Expression::StaticMemberExpression(member) => member.property.name.as_str(),
                _ => "",
            };
            matches!(wrapper, "memo" | "forwardRef")
                && call
                    .arguments
                    .first()
                    .and_then(Argument::as_expression)
                    .is_some_and(function_value)
        }
        _ => false,
    }
}

/// Whether the local name an `export { name }` exports is declared as a function in `file`.
fn local_function(file: &SourceFile<'_>, local: &ModuleExportName<'_>) -> bool {
    let ModuleExportName::IdentifierReference(identifier) = local else {
        return false;
    };
    let semantic = file.semantic();
    let Some(symbol) = identifier
        .reference_id
        .get()
        .and_then(|reference| semantic.scoping().get_reference(reference).symbol_id())
    else {
        return false;
    };
    let node = semantic.symbol_declaration(symbol);
    match node.kind() {
        AstKind::Function(_) | AstKind::Class(_) => true,
        AstKind::VariableDeclarator(declarator) => {
            matches!(declarator.id, BindingPattern::BindingIdentifier(_))
                && declarator.init.as_ref().is_some_and(function_value)
        }
        _ => false,
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

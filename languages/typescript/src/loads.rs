//! The loads of one file, each with its form, the names it takes and its line: what a rule needs to
//! say which module a file should have named.
//!
//! [`file_loads`] reads the top-level import and export declarations with a source and
//! `import x = require(...)`, and, anywhere in the file, `import(...)` types, dynamic `import(...)`,
//! calls of a `require` function with one string argument, and calls of the dotted names a rule
//! asks for (such as a test runner's mock functions), whose first argument is a string.

use oxc_ast::ast::{
    Argument, AwaitExpression, BindingPattern, CallExpression, Expression,
    ImportDeclarationSpecifier, ImportExpression, ImportOrExportKind, Statement, TSImportType,
    TSImportTypeQualifier, TSModuleReference, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use super::SourceFile;
use super::binding::is_require;
use super::modules::is_type_only_import;
use super::owners::export_name;
use super::value::string_like;

/// How a file loads a module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Form {
    /// `import … from "x"`, with a clause (`import {} from "x"` included).
    Import,
    /// `import "x"`: no clause.
    SideEffect,
    /// `export { … } from "x"`.
    ExportFrom,
    /// `export * from "x"`.
    ExportAll,
    /// `export * as ns from "x"`.
    ExportAllAs,
    /// `import x = require("x")`.
    ImportEquals,
    /// An `import("x")` type: `typeof import("x")`, `import("x").T`.
    ImportType,
    /// A dynamic `import("x")`.
    Dynamic,
    /// A call of a `require` function with one string argument.
    Require,
    /// A call of the dotted name at this index of the names [`file_loads`] was given.
    Call(usize),
}

/// The names a load takes from its module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Names {
    /// The names listed, as the module exports them (`a as b` takes `a`; a default import takes
    /// `default`). Empty for a side-effect import or an empty clause.
    Listed(Vec<String>),
    /// Every name the module exports: a namespace import, `export *`, `export * as`, `import x =
    /// require`, `require`, an `import()` that is not destructured, `typeof import()`.
    All,
}

/// One load of a module by a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Load {
    /// The specifier as written.
    pub specifier: String,
    /// How the module is loaded.
    pub form: Form,
    /// The names the load takes.
    pub names: Names,
    /// Whether the load takes types only: `import type`, `export type`, a clause whose every named
    /// element is `type`, `import type x = require`, and every `import()` type.
    pub type_only: bool,
    /// The one-based line of the declaration or expression.
    pub line: usize,
    /// The byte offset it starts at.
    pub start: u32,
}

/// Every load of `file`, in source order. `calls` are dotted callee names (`a.b`) whose calls count
/// as loads of their first argument when it is a string.
#[must_use]
pub fn file_loads(file: &SourceFile<'_>, calls: &[&str]) -> Vec<Load> {
    let mut finder = Finder {
        file,
        calls,
        loads: Vec::new(),
        destructured: Vec::new(),
    };

    for statement in &file.program().body {
        finder.statement(statement);
    }
    finder.visit_program(file.program());
    finder.loads.sort_by_key(|load| load.start);
    finder.loads
}

/// The loads of `file`'s top-level declarations, in source order: imports, side-effect imports,
/// `export … from`, `export *`, `export * as` and `import x = require`. The loads of
/// [`file_loads`] in those forms, read without walking expressions or building the semantic model.
#[must_use]
pub fn static_loads(file: &SourceFile<'_>) -> Vec<Load> {
    let mut finder = Finder {
        file,
        calls: &[],
        loads: Vec::new(),
        destructured: Vec::new(),
    };
    for statement in &file.program().body {
        finder.statement(statement);
    }
    finder.loads.sort_by_key(|load| load.start);
    finder.loads
}

struct Finder<'f, 'a> {
    file: &'f SourceFile<'a>,
    calls: &'f [&'f str],
    loads: Vec<Load>,
    /// The starts of the `import()` expressions already read as destructured.
    destructured: Vec<u32>,
}

impl Finder<'_, '_> {
    fn push(&mut self, specifier: &str, form: Form, names: Names, type_only: bool, start: u32) {
        self.loads.push(Load {
            specifier: specifier.to_owned(),
            form,
            names,
            type_only,
            line: usize::try_from(self.file.line(start)).unwrap_or(usize::MAX),
            start,
        });
    }

    /// The load of a top-level declaration.
    fn statement(&mut self, statement: &Statement<'_>) {
        match statement {
            Statement::ImportDeclaration(import) => {
                let type_only = is_type_only_import(import);
                let (form, names) = match &import.specifiers {
                    None => (Form::SideEffect, Names::Listed(Vec::new())),
                    Some(specifiers) => (
                        Form::Import,
                        specifiers
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
                            .collect::<Option<Vec<_>>>()
                            .map_or(Names::All, Names::Listed),
                    ),
                };
                self.push(
                    &import.source.value,
                    form,
                    names,
                    type_only,
                    import.span.start,
                );
            }
            Statement::ExportFromDeclaration(export) => {
                let type_only = export.export_kind == ImportOrExportKind::Type
                    || (!export.specifiers.is_empty()
                        && export
                            .specifiers
                            .iter()
                            .all(|specifier| specifier.export_kind == ImportOrExportKind::Type));
                self.push(
                    &export.source.value,
                    Form::ExportFrom,
                    Names::Listed(
                        export
                            .specifiers
                            .iter()
                            .map(|specifier| export_name(&specifier.local))
                            .collect(),
                    ),
                    type_only,
                    export.span.start,
                );
            }
            Statement::ExportAllDeclaration(export) => self.push(
                &export.source.value,
                if export.exported.is_some() {
                    Form::ExportAllAs
                } else {
                    Form::ExportAll
                },
                Names::All,
                export.export_kind == ImportOrExportKind::Type,
                export.span.start,
            ),
            Statement::TSImportEqualsDeclaration(import) => {
                if let TSModuleReference::ExternalModuleReference(reference) =
                    &import.module_reference
                {
                    self.push(
                        &reference.expression.value,
                        Form::ImportEquals,
                        Names::All,
                        import.import_kind == ImportOrExportKind::Type,
                        import.span.start,
                    );
                }
            }
            _ => {}
        }
    }

    /// The index in `calls` of the dotted name `callee` spells.
    fn call_index(&self, callee: &Expression<'_>) -> Option<usize> {
        let name = dotted(callee)?;
        self.calls.iter().position(|call| *call == name)
    }
}

impl<'a> Visit<'a> for Finder<'_, 'a> {
    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if let (BindingPattern::ObjectPattern(pattern), Some(Expression::AwaitExpression(wait))) =
            (&declarator.id, declarator.init.as_ref())
            && let Some(import) = awaited_import(wait)
            && let Some(specifier) = string_like(&import.source)
        {
            let names = if pattern.rest.is_some() {
                None
            } else {
                pattern
                    .properties
                    .iter()
                    .map(|property| {
                        (!property.computed)
                            .then(|| property.key.static_name().map(|name| name.to_string()))
                            .flatten()
                    })
                    .collect::<Option<Vec<_>>>()
            };
            self.push(
                &specifier,
                Form::Dynamic,
                names.map_or(Names::All, Names::Listed),
                false,
                import.span.start,
            );
            self.destructured.push(import.span.start);
        }

        walk::walk_variable_declarator(self, declarator);
    }

    fn visit_ts_import_type(&mut self, import: &TSImportType<'a>) {
        let names = import.qualifier.as_ref().map_or(Names::All, |qualifier| {
            Names::Listed(vec![first_segment(qualifier)])
        });
        self.push(
            &import.source.value,
            Form::ImportType,
            names,
            true,
            import.span.start,
        );
        walk::walk_ts_import_type(self, import);
    }

    fn visit_import_expression(&mut self, import: &ImportExpression<'a>) {
        if import.options.is_none()
            && import.phase.is_none()
            && !self.destructured.contains(&import.span.start)
            && let Some(specifier) = string_like(&import.source)
        {
            self.push(
                &specifier,
                Form::Dynamic,
                Names::All,
                false,
                import.span.start,
            );
        }

        walk::walk_import_expression(self, import);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let Some(index) = self.call_index(&call.callee) {
            if let Some(specifier) = call.arguments.first().and_then(argument_text) {
                self.push(
                    &specifier,
                    Form::Call(index),
                    Names::Listed(Vec::new()),
                    false,
                    call.span.start,
                );
            }
        } else if let [argument] = call.arguments.as_slice()
            && let Some(specifier) = argument_text(argument)
            && {
                // The semantic model, built on the first such call, resolves the references the
                // `require` test reads.
                let _ = self.file.semantic();
                is_require(self.file, &call.callee)
            }
        {
            self.push(
                &specifier,
                Form::Require,
                Names::All,
                false,
                call.span.start,
            );
        }

        walk::walk_call_expression(self, call);
    }
}

/// The `import()` an `await` waits for, through parentheses.
fn awaited_import<'e, 'a>(wait: &'e AwaitExpression<'a>) -> Option<&'e ImportExpression<'a>> {
    let mut expression = &wait.argument;
    while let Expression::ParenthesizedExpression(inner) = expression {
        expression = &inner.expression;
    }
    match expression {
        Expression::ImportExpression(import)
            if import.options.is_none() && import.phase.is_none() =>
        {
            Some(import)
        }
        _ => None,
    }
}

/// The first name of an `import()` type's qualifier: `b` of `import("./a").b.c`.
fn first_segment(qualifier: &TSImportTypeQualifier<'_>) -> String {
    match qualifier {
        TSImportTypeQualifier::Identifier(name) => name.name.to_string(),
        TSImportTypeQualifier::QualifiedName(name) => first_segment(&name.left),
    }
}

/// The dotted name an identifier or a chain of static member accesses on one spells: `vi.mock`.
fn dotted(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier.name.to_string()),
        Expression::StaticMemberExpression(member) if !member.optional => Some(format!(
            "{}.{}",
            dotted(&member.object)?,
            member.property.name
        )),
        _ => None,
    }
}

fn argument_text(argument: &Argument<'_>) -> Option<String> {
    argument.as_expression().and_then(string_like)
}

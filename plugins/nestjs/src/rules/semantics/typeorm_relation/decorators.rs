//! The `TypeORM` decorators of one file: those imported by name, under an alias or through a
//! namespace import, resolved by symbol so that a shadowing local name or another module's
//! decorator of the same name is not one.

use std::collections::{HashMap, HashSet};

use gate_typescript::SourceFile;
use gate_typescript::binder::{Binder, Meaning};
use gate_typescript::oxc_ast::ast::{
    CallExpression, Decorator, Expression, ImportDeclarationSpecifier, ImportOrExportKind,
    ModuleExportName, Program, Statement,
};
use gate_typescript::oxc_syntax::symbol::SymbolId;

use super::Vocabulary;

/// The name a module export name spells.
pub(super) fn export_name<'a>(name: &'a ModuleExportName<'a>) -> &'a str {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.as_str(),
        ModuleExportName::IdentifierReference(name) => name.name.as_str(),
        ModuleExportName::StringLiteral(literal) => literal.value.as_str(),
    }
}

/// The runtime `TypeORM` imports of a file.
#[derive(Debug, Default)]
pub(super) struct Bindings<'v> {
    named: HashMap<SymbolId, &'v str>,
    namespaces: HashSet<SymbolId>,
}

/// The runtime `TypeORM` import specifiers of a program that bind a namespace or a recognized
/// decorator, with the decorator each names.
fn specifiers<'p, 'a, 'v>(
    program: &'p Program<'a>,
    vocabulary: &'v Vocabulary,
) -> impl Iterator<Item = (&'p ImportDeclarationSpecifier<'a>, Option<&'v str>)> {
    program
        .body
        .iter()
        .filter_map(move |statement| match statement {
            Statement::ImportDeclaration(import)
                if import.source.value == vocabulary.package.as_str()
                    && import.import_kind != ImportOrExportKind::Type =>
            {
                import.specifiers.as_ref()
            }
            _ => None,
        })
        .flatten()
        .filter_map(move |specifier| match specifier {
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => Some((specifier, None)),
            ImportDeclarationSpecifier::ImportSpecifier(named)
                if named.import_kind != ImportOrExportKind::Type =>
            {
                vocabulary
                    .recognized(export_name(&named.imported))
                    .map(|canonical| (specifier, Some(canonical)))
            }
            _ => None,
        })
}

impl<'v> Bindings<'v> {
    /// The bindings of a file, or `None` when it imports no namespace or decorator the check reads:
    /// such a file has no entity, column or relation. Only a file with one gets its semantic model,
    /// which binds the import names to symbols.
    pub(super) fn read(file: &SourceFile<'_>, vocabulary: &'v Vocabulary) -> Option<Self> {
        let program = file.program();
        specifiers(program, vocabulary).next()?;
        // Building the semantic model binds the import names to their symbols.
        let _ = file.semantic();
        let mut bindings = Self::default();

        for (specifier, canonical) in specifiers(program, vocabulary) {
            match (specifier, canonical) {
                (ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace), _) => {
                    bindings.namespaces.extend(namespace.local.symbol_id.get());
                }
                (ImportDeclarationSpecifier::ImportSpecifier(named), Some(canonical)) => {
                    if let Some(symbol) = named.local.symbol_id.get() {
                        bindings.named.insert(symbol, canonical);
                    }
                }
                _ => {}
            }
        }

        Some(bindings)
    }
}

/// A decorator that names a recognized `TypeORM` decorator, with its call.
#[derive(Clone, Copy, Debug)]
pub(super) struct Resolved<'d, 'a, 'v> {
    pub canonical: &'v str,
    pub call: Option<&'d CallExpression<'a>>,
}

/// `@Name(...)` or `@Name` bound to a `TypeORM` import, or
/// `@namespace.Name(...)` on a `TypeORM` namespace import.
pub(super) fn resolve<'d, 'a, 'v>(
    decorator: &'d Decorator<'a>,
    bindings: &Bindings<'v>,
    binder: &Binder<'_, 'a>,
    vocabulary: &'v Vocabulary,
) -> Option<Resolved<'d, 'a, 'v>> {
    let call = match &decorator.expression {
        Expression::CallExpression(call) => Some(&**call),
        _ => None,
    };
    let callee = call.map_or(&decorator.expression, |call| &call.callee);
    let canonical = match callee {
        Expression::Identifier(identifier) => binder
            .resolve(identifier, Meaning::Value)
            .and_then(|symbol| bindings.named.get(&symbol).copied()),
        Expression::StaticMemberExpression(member) if !member.optional => {
            let canonical = vocabulary.recognized(member.property.name.as_str())?;
            let Expression::Identifier(namespace) = &member.object else {
                return None;
            };

            binder
                .resolve(namespace, Meaning::Value)
                .filter(|symbol| bindings.namespaces.contains(symbol))
                .map(|_| canonical)
        }
        _ => None,
    }?;

    Some(Resolved { canonical, call })
}

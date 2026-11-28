//! Lock values: what a `mode` folds to without running code. A literal, a `const` binding or an
//! import folds through `ValueReader`; an enum member folds to its string initializer; a
//! conditional folds to each of its branches; and an identifier whose declaration has a written
//! type annotation naming an enum folds to every member of that enum.

use gate_typescript::SourceFile;
use gate_typescript::owners::{top_level, unwrap};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Declaration, Expression, IdentifierReference, Statement, TSEnumDeclaration, TSEnumMemberName,
    TSType, TSTypeAnnotation, TSTypeName, VariableDeclarationKind,
};
use gate_typescript::value::{StaticValue, ValueReader, string_like};

use super::{Context, owner};

/// How deep `const` bindings are followed.
const DEPTH: usize = 8;

/// What one branch of a value folds to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Branch {
    /// A known string.
    Known(String),
    /// Any member of an enum, through a declared type: the members' string values, or `None` for a
    /// member without one.
    Declared(Vec<Option<String>>),
    /// Not known.
    Unknown,
}

/// What an identifier is bound to in its file.
pub(crate) enum Binding<'a> {
    /// A variable: whether it is `const`, its initializer and its written type.
    Variable {
        constant: bool,
        init: Option<&'a Expression<'a>>,
        annotation: Option<&'a TSTypeAnnotation<'a>>,
    },
    /// A parameter and its written type.
    Parameter(Option<&'a TSTypeAnnotation<'a>>),
    /// An enum declared in the file.
    Enum(&'a TSEnumDeclaration<'a>),
    /// An import.
    Import,
    /// Anything else.
    Other,
}

/// The one declaration an identifier reference resolves to through the file's scopes.
pub(crate) fn binding<'a>(
    file: &SourceFile<'a>,
    identifier: &IdentifierReference<'_>,
) -> Option<Binding<'a>> {
    let semantic = file.semantic();
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let symbol = scoping
        .get_reference(identifier.reference_id.get()?)
        .symbol_id()?;
    let declarations: Vec<_> = scoping.symbol_declarations(symbol).collect();
    let [declaration] = declarations.as_slice() else {
        return Some(Binding::Other);
    };
    let kind = |kind: AstKind<'a>| match kind {
        AstKind::VariableDeclarator(declarator) => Some(Binding::Variable {
            constant: matches!(
                nodes.parent_kind(declarator.node_id.get()),
                AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
            ),
            init: declarator.init.as_ref(),
            annotation: declarator.type_annotation.as_deref(),
        }),
        AstKind::FormalParameter(parameter) => {
            Some(Binding::Parameter(parameter.type_annotation.as_deref()))
        }
        AstKind::TSEnumDeclaration(declaration) => Some(Binding::Enum(declaration)),
        AstKind::ImportSpecifier(_)
        | AstKind::ImportDefaultSpecifier(_)
        | AstKind::ImportNamespaceSpecifier(_) => Some(Binding::Import),
        _ => None,
    };

    Some(
        kind(nodes.kind(*declaration))
            .or_else(|| kind(nodes.parent_kind(*declaration)))
            .unwrap_or(Binding::Other),
    )
}

/// The members of the enum an identifier names, through a local declaration or an import chased
/// to its declaration.
fn enum_members<'a>(
    context: &Context<'_, 'a>,
    file: &SourceFile<'a>,
    identifier: &IdentifierReference<'a>,
) -> Option<Vec<(String, Option<String>)>> {
    match binding(file, identifier)? {
        Binding::Enum(declaration) => Some(members(declaration)),
        Binding::Import => {
            let (target, name) = owner::imported(context, file, identifier, false)?;
            let declared = context.declaration(&target, &name)?;
            let path = declared.path?;
            let parsed = context.typescript.parse(&path).ok().flatten()?;

            top_level(&parsed).find_map(|statement| {
                let declaration = match statement {
                    Statement::TSEnumDeclaration(declaration) => declaration,
                    Statement::ExportDeclaration(export) => match &export.declaration {
                        Declaration::TSEnumDeclaration(declaration) => declaration,
                        _ => return None,
                    },
                    _ => return None,
                };

                (declaration.id.name == declared.name.as_str()).then(|| members(declaration))
            })
        }
        _ => None,
    }
}

/// The names and string values of an enum's members.
fn members(declaration: &TSEnumDeclaration<'_>) -> Vec<(String, Option<String>)> {
    declaration
        .body
        .members
        .iter()
        .map(|member| {
            let name = match &member.id {
                TSEnumMemberName::Identifier(identifier) => identifier.name.to_string(),
                TSEnumMemberName::String(literal) | TSEnumMemberName::ComputedString(literal) => {
                    literal.value.to_string()
                }
                TSEnumMemberName::ComputedTemplateString(template) => template
                    .quasis
                    .first()
                    .and_then(|quasi| quasi.value.cooked.as_ref())
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            };

            (name, member.initializer.as_ref().and_then(string_like))
        })
        .collect()
}

/// The branches `expression` folds to.
pub(crate) fn fold<'a>(
    context: &Context<'_, 'a>,
    file: &SourceFile<'a>,
    expression: &Expression<'a>,
) -> Vec<Branch> {
    let mut branches = Vec::new();
    fold_into(context, file, expression, 0, &mut branches);
    branches
}

fn fold_into<'a>(
    context: &Context<'_, 'a>,
    file: &SourceFile<'a>,
    expression: &Expression<'a>,
    depth: usize,
    branches: &mut Vec<Branch>,
) {
    let value = unwrap(expression);

    if depth > DEPTH {
        branches.push(Branch::Unknown);
        return;
    }

    match value {
        Expression::ConditionalExpression(conditional) => {
            fold_into(context, file, &conditional.consequent, depth + 1, branches);
            fold_into(context, file, &conditional.alternate, depth + 1, branches);
            return;
        }
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(object) = unwrap(&member.object)
                && let Some(members) = enum_members(context, file, object)
            {
                branches.push(
                    members
                        .into_iter()
                        .find(|(name, _)| *name == member.property.name.as_str())
                        .and_then(|(_, value)| value)
                        .map_or(Branch::Unknown, Branch::Known),
                );
                return;
            }
        }
        Expression::Identifier(identifier) => match binding(file, identifier) {
            Some(Binding::Variable {
                constant: true,
                init: Some(init),
                ..
            }) => {
                fold_into(context, file, init, depth + 1, branches);
                return;
            }
            Some(
                Binding::Variable {
                    annotation: Some(annotation),
                    ..
                }
                | Binding::Parameter(Some(annotation)),
            ) => {
                if let Some(values) = declared(context, file, &annotation.type_annotation) {
                    branches.push(Branch::Declared(values));
                    return;
                }
            }
            _ => {}
        },
        _ => {}
    }

    let read = ValueReader::new(context.typescript).read(file, value);

    branches.push(match read {
        Ok(Some(StaticValue::String(text))) => Branch::Known(text),
        _ => Branch::Unknown,
    });
}

/// The member values of the enum a written type names.
fn declared<'a>(
    context: &Context<'_, 'a>,
    file: &SourceFile<'a>,
    kind: &TSType<'a>,
) -> Option<Vec<Option<String>>> {
    let TSType::TSTypeReference(reference) = kind else {
        return None;
    };
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return None;
    };

    enum_members(context, file, name)
        .map(|members| members.into_iter().map(|(_, value)| value).collect())
}

/// Whether the branches hold a row-lock mode: a known value of `lock-modes`, or a declared enum
/// whose every member is one.
pub(crate) fn locks_rows(context: &Context<'_, '_>, branches: &[Branch]) -> bool {
    let listed = |value: &str| context.lock_modes.iter().any(|mode| mode == value);

    branches.iter().any(|branch| match branch {
        Branch::Known(value) => listed(value),
        Branch::Declared(values) => {
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| value.as_deref().is_some_and(listed))
        }
        Branch::Unknown => false,
    })
}

/// Whether every branch is a known mode that locks no row.
pub(crate) fn row_free(context: &Context<'_, '_>, branches: &[Branch]) -> bool {
    !branches.is_empty()
        && branches.iter().all(|branch| {
            matches!(branch, Branch::Known(value) if context.row_free_modes.contains(&value.as_str()))
        })
}

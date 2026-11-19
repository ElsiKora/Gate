//! The declared types the relations are compared by: the relation property's target type and the
//! closed scalar type of a join column, read from the type annotations as the TypeScript AST spells
//! them.

use gate_typescript::binder::Meaning;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    ClassElement, IdentifierReference, ImportOrExportKind, TSType, TSTypeName,
};
use gate_typescript::oxc_syntax::node::NodeId;

use super::catalog::{Catalog, Kind};

/// The scalar type of a join column: its identity and whether it admits `null`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Scalar {
    pub identity: String,
    pub nullable: bool,
}

/// The declared type of a property that is neither optional nor untyped, as
/// `ts.isPropertyDeclaration(member) && !member.questionToken && member.type` read it.
pub(super) fn declared<'e, 'a>(member: &'e ClassElement<'a>) -> Option<&'e TSType<'a>> {
    match member {
        ClassElement::PropertyDefinition(property) if !property.optional => property
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.type_annotation),
        ClassElement::AccessorProperty(property) => property
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.type_annotation),
        _ => None,
    }
}

/// A reference to `name` without type arguments.
fn names(kind: &TSType<'_>, name: &str) -> bool {
    matches!(kind, TSType::TSTypeReference(reference)
        if reference.type_arguments.is_none()
            && matches!(&reference.type_name, TSTypeName::IdentifierReference(identifier) if identifier.name == name))
}

/// How a relation property spells its target type: whether it admits `null` and whether `Target`
/// is wrapped as `Relation<Target>` through the unaliased named `Relation` import of `typeorm`.
pub(super) struct TargetType {
    pub nullable: bool,
    pub wrapped: bool,
}

/// Exactly `Target`, or `Target` as the single type argument of a `Relation` wrapper: the name
/// `Relation`, a name that a named import of `Relation` binds, or a qualified name ending in
/// `Relation`. Only the unaliased `Relation` imported from the `TypeORM` package counts as wrapped; its erased
/// metadata keeps the class from being read before an import cycle has defined it under native ESM.
/// `None` for any other type.
fn target_form(
    catalog: &Catalog<'_, '_, '_>,
    unit: usize,
    kind: &TSType<'_>,
    target: &str,
) -> Option<TargetType> {
    if names(kind, target) {
        return Some(TargetType {
            nullable: false,
            wrapped: false,
        });
    }

    let TSType::TSTypeReference(reference) = kind else {
        return None;
    };
    let Some([argument]) = reference
        .type_arguments
        .as_ref()
        .map(|arguments| arguments.params.as_slice())
    else {
        return None;
    };

    if !names(argument, target) {
        return None;
    }

    let wrapped = match &reference.type_name {
        TSTypeName::IdentifierReference(identifier) => {
            let import = relation_import(catalog, unit, identifier);

            if identifier.name != "Relation"
                && import.is_none_or(|(_, imported, _)| imported != "Relation")
            {
                return None;
            }

            identifier.name == "Relation"
                && import.is_some_and(|(module, imported, _)| {
                    module == catalog.vocabulary.package && imported == "Relation"
                })
        }
        TSTypeName::QualifiedName(name) if name.right.name == "Relation" => false,
        _ => return None,
    };

    Some(TargetType {
        nullable: false,
        wrapped,
    })
}

/// Exactly `Target`, or `Target | null` in either order, otherwise
/// `None`; `Target` may be wrapped as `Relation<Target>` and parentheses are not looked through.
pub(super) fn target_type(
    catalog: &Catalog<'_, '_, '_>,
    unit: usize,
    kind: &TSType<'_>,
    target: &str,
) -> Option<TargetType> {
    if let Some(form) = target_form(catalog, unit, kind, target) {
        return Some(form);
    }

    let TSType::TSUnionType(union) = kind else {
        return None;
    };
    let form = match union.types.as_slice() {
        [first, TSType::TSNullKeyword(_)] => target_form(catalog, unit, first, target)?,
        [TSType::TSNullKeyword(_), second] => target_form(catalog, unit, second, target)?,
        _ => return None,
    };

    Some(TargetType {
        nullable: true,
        ..form
    })
}

/// The named import that the type name `name` of the unit `unit` resolves to, as `import_of` reads
/// it; `None` when it resolves to anything else.
fn relation_import<'a>(
    catalog: &Catalog<'_, 'a, '_>,
    unit: usize,
    name: &IdentifierReference<'_>,
) -> Option<(&'a str, &'a str, bool)> {
    let (_, binder) = catalog.units[unit].resolver.as_ref()?;
    let declarations = binder
        .resolve(name, Meaning::Type)
        .map(|symbol| binder.declarations(symbol))
        .unwrap_or_default();

    match declarations.as_slice() {
        [id] => import_of(catalog, unit, *id),
        _ => None,
    }
}

/// A type without its parentheses.
fn unparenthesized<'t, 'a>(kind: &'t TSType<'a>) -> &'t TSType<'a> {
    let mut current = kind;

    while let TSType::TSParenthesizedType(inner) = current {
        current = &inner.type_annotation;
    }

    current
}

/// A primitive, an intrinsic `Date` or a repository enum, alone or with
/// `| null`, of a non-optional property of the unit `unit`.
pub(super) fn scalar(
    catalog: &Catalog<'_, '_, '_>,
    unit: usize,
    member: &ClassElement<'_>,
) -> Option<Scalar> {
    let declared = unparenthesized(declared(member)?);
    let (base, nullable) = match declared {
        TSType::TSUnionType(union) => {
            let types: Vec<&TSType<'_>> = union.types.iter().map(unparenthesized).collect();
            let nulls = types
                .iter()
                .filter(|kind| matches!(kind, TSType::TSNullKeyword(_)))
                .count();

            if types.len() != 2 || nulls != 1 {
                return None;
            }

            (
                *types
                    .iter()
                    .find(|kind| !matches!(kind, TSType::TSNullKeyword(_)))?,
                true,
            )
        }
        other => (other, false),
    };
    let primitive = match base {
        TSType::TSBigIntKeyword(_) => Some("bigint"),
        TSType::TSBooleanKeyword(_) => Some("boolean"),
        TSType::TSNumberKeyword(_) => Some("number"),
        TSType::TSStringKeyword(_) => Some("string"),
        _ => None,
    };

    if let Some(primitive) = primitive {
        return Some(Scalar {
            identity: format!("primitive:{primitive}"),
            nullable,
        });
    }

    let TSType::TSTypeReference(reference) = base else {
        return None;
    };
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return None;
    };

    if reference.type_arguments.is_some() {
        return None;
    }

    let (_, binder) = catalog.units[unit].resolver.as_ref()?;

    // `Date` is intrinsic when no declaration of this file or global one names it.
    if name.name == "Date" {
        return (binder.resolve(name, Meaning::Type).is_none() && !catalog.global_date).then(
            || Scalar {
                identity: "intrinsic:Date".to_owned(),
                nullable,
            },
        );
    }

    enum_identity(catalog, unit, name).map(|identity| Scalar {
        identity: format!("enum:{identity}"),
        nullable,
    })
}

/// An exported top-level enum of this unit, or the one enum a named import
/// reaches through explicit exports.
fn enum_identity(
    catalog: &Catalog<'_, '_, '_>,
    unit: usize,
    name: &IdentifierReference<'_>,
) -> Option<String> {
    let (_, binder) = catalog.units[unit].resolver.as_ref()?;
    let nodes = catalog.units[unit].file.semantic().nodes();
    let declarations = binder
        .resolve(name, Meaning::Type)
        .map(|symbol| binder.declarations(symbol))
        .unwrap_or_default();
    let enums: Vec<_> = declarations
        .iter()
        .filter_map(|&id| match nodes.kind(id) {
            AstKind::TSEnumDeclaration(declaration) => Some(declaration),
            _ => None,
        })
        .collect();

    if let [declaration] = enums.as_slice()
        && let Some(identity) = catalog.enum_identity(unit, declaration)
    {
        return Some(identity);
    }

    let imports: Vec<_> = declarations
        .iter()
        .filter_map(|&id| import_of(catalog, unit, id))
        .collect();
    let [(module, imported, _)] = imports.as_slice() else {
        return None;
    };
    let source = catalog.imported(catalog.units[unit].path, module)?;
    let found = catalog.exported(source, imported, Kind::Enum, false, &[]);

    match found.as_slice() {
        [(_, identity)] => Some(identity.clone()),
        _ => None,
    }
}

/// The module, imported name and type-only state of a declaration node that is a named import
/// specifier; `None` for any other declaration.
pub(super) fn import_of<'a>(
    catalog: &Catalog<'_, 'a, '_>,
    unit: usize,
    id: NodeId,
) -> Option<(&'a str, &'a str, bool)> {
    let nodes = catalog.units[unit].file.semantic().nodes();
    let AstKind::ImportSpecifier(specifier) = nodes.kind(id) else {
        return None;
    };
    let AstKind::ImportDeclaration(import) = nodes.parent_kind(id) else {
        return None;
    };
    let type_only = import.import_kind == ImportOrExportKind::Type
        || specifier.import_kind == ImportOrExportKind::Type;

    Some((
        import.source.value.as_str(),
        super::decorators::export_name(&specifier.imported),
        type_only,
    ))
}

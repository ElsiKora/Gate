//! The configuration and secret contracts that reference a configuration key.
//!
//! A `CODE` entry of kind `CONFIG_KEY` names under `reference` each contract by which environment
//! configuration or secrets locate its record, as `<path>: <Type>.<member>[.<member>…]`: a
//! required property typed `string` of a top-level interface or object type alias (all its merged
//! declarations) of a tracked `.ts` source file below a configuration or secret root, reached from
//! the type through required properties whose types are inline object types or arrays of them
//! (`T[]`, `readonly T[]`, `Array<T>`, `ReadonlyArray<T>`). Properties inherited through `extends`
//! and named types are not followed. The check proves the contract property exists; whether it
//! carries the key is reviewed.

use gate_sdk::Result;
use gate_typescript::TypeScript;
use gate_typescript::oxc_ast::ast::{
    Declaration, ExportDefaultDeclarationKind, Statement, TSSignature, TSType, TSTypeName,
    TSTypeOperatorOperator,
};

use crate::shared::decorators::key_name;

/// Why `source` names no string property of a configuration or secret contract, or `None` when it
/// does.
///
/// # Errors
/// Fails when a tracked file cannot be read.
pub(crate) fn problem(
    typescript: &TypeScript<'_>,
    roots: &[String],
    source: &str,
) -> Result<Option<String>> {
    let Some((path, chain)) = source.split_once(": ") else {
        return Ok(Some("is not <path>: <Type>.<member>".to_owned()));
    };

    if !roots.iter().any(|root| path.starts_with(root.as_str())) {
        return Ok(Some(format!(
            "lies outside the configuration and secret roots {}",
            roots.join(", ")
        )));
    }

    let mut names = chain.split('.');
    let type_name = names.next().unwrap_or_default();
    let members: Vec<&str> = names.collect();

    if type_name.is_empty() || members.is_empty() || members.iter().any(|name| name.is_empty()) {
        return Ok(Some("is not <path>: <Type>.<member>".to_owned()));
    }

    let file_name = std::path::Path::new(path);

    if file_name
        .extension()
        .is_none_or(|extension| extension != "ts")
        || file_name
            .file_stem()
            .and_then(|stem| std::path::Path::new(stem).extension())
            .is_some_and(|extension| extension == "d")
    {
        return Ok(Some(format!(
            "names {path}, which is no TypeScript source file"
        )));
    }

    let Some(file) = typescript.parse(path)? else {
        return Ok(Some(format!("names {path}, which is no tracked file")));
    };

    if file.fatal() {
        return Ok(Some(format!("names {path}, which cannot be parsed")));
    }

    // Declarations of one name merge, so the type's members are those of all of them.
    let mut signatures: Vec<&[TSSignature<'_>]> = file
        .program()
        .body
        .iter()
        .filter_map(|statement| {
            let (name, signatures) = declared(statement)?;
            (name == type_name).then_some(signatures)
        })
        .collect();

    if signatures.is_empty() {
        return Ok(Some(format!(
            "names {type_name}, which {path} declares as no interface or object type alias"
        )));
    }

    for (index, member) in members.iter().enumerate() {
        let reached = members[..=index].join(".");
        let Some(property) = signatures
            .iter()
            .flat_map(|list| list.iter())
            .find_map(|signature| match signature {
                TSSignature::TSPropertySignature(property)
                    if key_name(&property.key, property.computed).as_deref() == Some(*member) =>
                {
                    Some(property)
                }
                _ => None,
            })
        else {
            return Ok(Some(format!(
                "names {type_name}.{reached}, which is no property"
            )));
        };

        if property.optional {
            return Ok(Some(format!(
                "names {type_name}.{reached}, which is optional"
            )));
        }

        let Some(annotation) = property
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.type_annotation)
        else {
            return Ok(Some(format!(
                "names {type_name}.{reached}, which has no type"
            )));
        };

        if index + 1 == members.len() {
            return Ok((!matches!(annotation, TSType::TSStringKeyword(_)))
                .then(|| format!("names {type_name}.{reached}, which is not typed string")));
        }

        let Some(next) = object(annotation) else {
            return Ok(Some(format!(
                "names {type_name}.{reached}, whose type is no inline object type or array of one"
            )));
        };
        signatures = vec![next];
    }

    Ok(None)
}

/// The name and members of the interface or object type alias a top-level statement declares,
/// exported or not.
fn declared<'s, 'a>(statement: &'s Statement<'a>) -> Option<(&'s str, &'s [TSSignature<'a>])> {
    let alias = |alias: &'s gate_typescript::oxc_ast::ast::TSTypeAliasDeclaration<'a>| match &alias
        .type_annotation
    {
        TSType::TSTypeLiteral(literal) => Some((alias.id.name.as_str(), &literal.members[..])),
        _ => None,
    };

    match statement {
        Statement::TSInterfaceDeclaration(interface) => {
            Some((interface.id.name.as_str(), &interface.body.body[..]))
        }
        Statement::TSTypeAliasDeclaration(declaration) => alias(declaration),
        Statement::ExportDeclaration(export) => match &export.declaration {
            Declaration::TSInterfaceDeclaration(interface) => {
                Some((interface.id.name.as_str(), &interface.body.body[..]))
            }
            Declaration::TSTypeAliasDeclaration(declaration) => alias(declaration),
            _ => None,
        },
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                Some((interface.id.name.as_str(), &interface.body.body[..]))
            }
            _ => None,
        },
        _ => None,
    }
}

/// The members of an inline object type, also as the element type of an array.
fn object<'s, 'a>(annotation: &'s TSType<'a>) -> Option<&'s [TSSignature<'a>]> {
    match annotation {
        TSType::TSTypeLiteral(literal) => Some(&literal.members[..]),
        TSType::TSParenthesizedType(inner) => object(&inner.type_annotation),
        TSType::TSArrayType(array) => object(&array.element_type),
        TSType::TSTypeOperatorType(operator)
            if operator.operator == TSTypeOperatorOperator::Readonly =>
        {
            object(&operator.type_annotation)
        }
        TSType::TSTypeReference(reference) => {
            let TSTypeName::IdentifierReference(name) = &reference.type_name else {
                return None;
            };
            let arguments = reference.type_arguments.as_ref()?;

            match (name.name.as_str(), arguments.params.as_slice()) {
                ("Array" | "ReadonlyArray", [element]) => object(element),
                _ => None,
            }
        }
        _ => None,
    }
}

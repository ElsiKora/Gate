//! The advisory-lock mint authority.
//!
//! The files that mint keys must keep the texts the `mint` option lists for them, and an object
//! literal with every key property (by default `family`, `familyId` and `resourceKey`), a
//! structural key, stands only in the files the option allows.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    AssignmentTargetProperty, ObjectPropertyKind, PropertyKey, PropertyKind,
};

/// The mint authority.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", default, deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Mint {
    /// The files that may construct a structural key.
    pub structural: Vec<String>,
    /// The texts a file must keep, in the order they are checked.
    pub required: Vec<Required>,
}

/// Texts one file must keep, and what its finding says when one is missing.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Required {
    pub path: String,
    pub texts: Vec<String>,
    /// The finding after the path, such as `does not retain the runtime dynamic-key mint.`.
    pub message: String,
}

/// Whether a list of property names holds every key property.
fn is_key<'a>(names: impl Iterator<Item = &'a str>, properties: &[String]) -> bool {
    let names: Vec<&str> = names.collect();

    properties
        .iter()
        .all(|property| names.contains(&property.as_str()))
}

/// Whether a syntax node is an object literal with every key property, as TypeScript reads an
/// object literal: an assigned or shorthand property with a plain name, also as the target of a
/// destructuring assignment.
fn is_structural_key(kind: AstKind<'_>, properties: &[String]) -> bool {
    match kind {
        AstKind::ObjectExpression(object) => is_key(
            object.properties.iter().filter_map(|property| {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    return None;
                };

                match &property.key {
                    PropertyKey::StaticIdentifier(name)
                        if property.kind == PropertyKind::Init
                            && !property.method
                            && !property.computed =>
                    {
                        Some(name.name.as_str())
                    }
                    _ => None,
                }
            }),
            properties,
        ),
        AstKind::ObjectAssignmentTarget(target) => is_key(
            target
                .properties
                .iter()
                .filter_map(|property| match property {
                    AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(identifier) => {
                        Some(identifier.binding.name.as_str())
                    }
                    AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
                        match &property.name {
                            PropertyKey::StaticIdentifier(name) if !property.computed => {
                                Some(name.name.as_str())
                            }
                            _ => None,
                        }
                    }
                }),
            properties,
        ),
        _ => false,
    }
}

/// Checks one file against the mint authority.
///
/// # Errors
/// Returns the message where the file drops a required text or constructs a structural key, an
/// object literal with every one of `properties`, outside the allowlist.
pub fn check(
    path: &str,
    text: &str,
    file: Option<&SourceFile<'_>>,
    mint: &Mint,
    properties: &[String],
) -> Result<(), String> {
    for required in mint
        .required
        .iter()
        .filter(|required| required.path == path)
    {
        if !required
            .texts
            .iter()
            .all(|needed| text.contains(needed.as_str()))
        {
            return Err(format!("{path} {}", required.message));
        }
    }

    let structural = file.is_some_and(|file| {
        file.semantic()
            .nodes()
            .iter()
            .any(|node| is_structural_key(node.kind(), properties))
    });

    if structural && !mint.structural.iter().any(|allowed| allowed == path) {
        return Err(format!(
            "{path} constructs a structural advisory-lock key outside the mint allowlist."
        ));
    }

    Ok(())
}

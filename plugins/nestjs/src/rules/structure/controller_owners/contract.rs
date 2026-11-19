//! The controller rows of the options, compared with JavaScript's value semantics: an entity that
//! a row leaves out is `null`, another field it leaves out is `undefined`, and a string is itself.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_typescript::owners::Owner;

pub(super) use crate::shared::js::Js;

/// `owner?.name === value` for a resolved owner.
pub(super) fn same_name(owner: Option<&Owner>, value: Js<'_>) -> bool {
    match owner {
        Some(owner) => value == Js::Str(&owner.name),
        None => value == Js::Undefined,
    }
}

/// `owner?.path === value` for a resolved owner, whose path is `null` outside the sources.
pub(super) fn same_path(owner: Option<&Owner>, value: Js<'_>) -> bool {
    match owner {
        Some(Owner {
            path: Some(path), ..
        }) => value == Js::Str(path),
        Some(Owner { path: None, .. }) => value == Js::Null,
        None => value == Js::Undefined,
    }
}

/// One row of `controllers`: a controller file and its semantic owner. The rule reports a row
/// that leaves out a field it needs, so every field may be left out; a `null` counts as left out,
/// except for the two entity fields, where `null` is the value of a `PLAIN` controller.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct ControllerRow {
    /// The controller file, ending in `/controller.ts`.
    #[serde(default)]
    pub path: Option<String>,
    /// The controller class, ending in `Controller`.
    #[serde(default)]
    pub controller_name: Option<String>,
    /// `CUSTOM_USE_CASE`, `ENTITY_RESOURCE`, `EXTERNAL_NOUN` or `PLAIN`.
    #[serde(default)]
    pub classification: Option<String>,
    /// The primary service class, ending in `Service`.
    #[serde(default)]
    pub primary_service_name: Option<String>,
    /// The file of the primary service class.
    #[serde(default)]
    pub primary_service_path: Option<String>,
    /// The entity the controller serves; `null` for a `PLAIN` controller.
    #[serde(default, deserialize_with = "present")]
    pub entity_name: Option<Nullable>,
    /// The file of the entity; `null` for a `PLAIN` controller.
    #[serde(default, deserialize_with = "present")]
    pub entity_path: Option<Nullable>,
    /// The entity of the primary service when it differs from `entity-name` (`CUSTOM_USE_CASE`
    /// only, with `primary-service-entity-path`).
    #[serde(default)]
    pub primary_service_entity_name: Option<String>,
    /// The file of that entity.
    #[serde(default)]
    pub primary_service_entity_path: Option<String>,
    /// The parent controller family the controller belongs to.
    #[serde(default)]
    pub semantic_owner_name: Option<String>,
    /// The accepted decision (`D-…`) behind the classification or the semantic owner.
    #[serde(default)]
    pub decision_id: Option<String>,
}

/// A string or `null`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", untagged)]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Nullable {
    /// A string.
    Text(String),
    /// `null`.
    Null,
}

/// Reads a field that is present, so that a `null` stays apart from a missing field (`None`).
fn present<'de, D>(deserializer: D) -> Result<Option<Nullable>, D::Error>
where
    D: gate_sdk::serde::Deserializer<'de>,
{
    Nullable::deserialize(deserializer).map(Some)
}

/// A field a row may leave out: `undefined` when it does.
fn optional(value: Option<&String>) -> Js<'_> {
    value.map_or(Js::Undefined, |text| Js::Str(text))
}

/// An entity field: `undefined` when left out, `null` when `null`.
fn entity(value: Option<&Nullable>) -> Js<'_> {
    match value {
        None => Js::Undefined,
        Some(Nullable::Null) => Js::Null,
        Some(Nullable::Text(text)) => Js::Str(text),
    }
}

/// One controller contract.
#[derive(Clone, Copy, Debug)]
pub(super) struct Contract<'v> {
    row: &'v ControllerRow,
}

impl<'v> Contract<'v> {
    pub(super) const fn new(row: &'v ControllerRow) -> Self {
        Self { row }
    }

    pub(super) fn path(self) -> Js<'v> {
        optional(self.row.path.as_ref())
    }

    pub(super) fn controller_name(self) -> Js<'v> {
        optional(self.row.controller_name.as_ref())
    }

    pub(super) fn classification(self) -> Js<'v> {
        optional(self.row.classification.as_ref())
    }

    pub(super) fn decision_id(self) -> Js<'v> {
        optional(self.row.decision_id.as_ref())
    }

    pub(super) fn own_service_entity_name(self) -> Js<'v> {
        optional(self.row.primary_service_entity_name.as_ref())
    }

    pub(super) fn own_service_entity_path(self) -> Js<'v> {
        optional(self.row.primary_service_entity_path.as_ref())
    }

    pub(super) fn plain(self) -> bool {
        self.classification() == Js::Str(PLAIN)
    }

    pub(super) fn entity_name(self) -> Js<'v> {
        entity(self.row.entity_name.as_ref())
    }

    pub(super) fn entity_path(self) -> Js<'v> {
        entity(self.row.entity_path.as_ref())
    }

    pub(super) fn service_entity_name(self) -> Js<'v> {
        self.own_service_entity_name().or(self.entity_name())
    }

    pub(super) fn service_entity_path(self) -> Js<'v> {
        self.own_service_entity_path().or(self.entity_path())
    }

    pub(super) fn primary_service_name(self) -> Js<'v> {
        optional(self.row.primary_service_name.as_ref())
    }

    pub(super) fn primary_service_path(self) -> Js<'v> {
        optional(self.row.primary_service_path.as_ref())
    }

    pub(super) fn semantic_owner_name(self) -> Js<'v> {
        optional(self.row.semantic_owner_name.as_ref())
    }
}

pub(super) const CUSTOM_USE_CASE: &str = "CUSTOM_USE_CASE";
pub(super) const ENTITY_RESOURCE: &str = "ENTITY_RESOURCE";
pub(super) const EXTERNAL_NOUN: &str = "EXTERNAL_NOUN";
pub(super) const PLAIN: &str = "PLAIN";

/// `/^D-[A-Z0-9][A-Z0-9-]*$/u.test(value ?? "")`.
pub(super) fn decision(value: Js<'_>) -> bool {
    let text = match value {
        Js::Str(text) => text,
        Js::Undefined | Js::Null => "",
        Js::Other(_) => return false,
    };
    let mut characters = text.chars();

    characters.next() == Some('D')
        && characters.next() == Some('-')
        && characters
            .next()
            .is_some_and(|first| first.is_ascii_uppercase() || first.is_ascii_digit())
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
        })
}

/// The path and controller class checks of one row: exact and not seen before.
fn unique<'v>(
    contract: Contract<'v>,
    label: &str,
    paths: &mut Vec<&'v str>,
    names: &mut Vec<&'v str>,
    report: &mut impl FnMut(&str, String),
) {
    match contract.path().string() {
        Some(path) if path.ends_with("/controller.ts") => {
            if paths.contains(&path) {
                report("path", format!("{label} duplicates controller path {path}"));
            } else {
                paths.push(path);
            }
        }
        _ => report(
            "path",
            format!("{label} must provide an exact controller.ts path"),
        ),
    }

    match contract.controller_name().string() {
        Some(name) if name.ends_with("Controller") => {
            if names.contains(&name) {
                report(
                    "class",
                    format!("{label} duplicates controller class {name}"),
                );
            } else {
                names.push(name);
            }
        }
        _ => report(
            "class",
            format!("{label} must provide a Controller class name"),
        ),
    }
}

/// The semantic owner of a row names a parent controller family and carries a decision.
fn semantic_owner(contract: Contract<'_>, label: &str, report: &mut impl FnMut(&str, String)) {
    let owner = contract.semantic_owner_name();

    if owner != Js::Undefined {
        let family = owner
            .string()
            .zip(contract.controller_name().string())
            .is_some_and(|(owner, controller)| {
                owner.starts_with(|first: char| first.is_ascii_uppercase())
                    && owner
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
                    && controller.starts_with(owner)
                    && owner.encode_utf16().count() + "Controller".len()
                        < controller.encode_utf16().count()
            });

        if !family {
            report(
                "semantic-owner",
                format!("{label} semanticOwnerName must identify a parent controller family"),
            );
        }

        if !decision(contract.decision_id()) {
            report(
                "semantic-owner-decision",
                format!("{label} semanticOwnerName requires an exact decisionId"),
            );
        }
    }
}

/// Reports the rows that break the registry's own contract, in row order, each as its key (the row's
/// path and the check) and its message. `external` is the folder prefix of the plane that holds
/// external nouns.
pub(super) fn validate(
    contracts: &[ControllerRow],
    external: &str,
    mut report: impl FnMut(String, String),
) {
    let mut paths: Vec<&str> = Vec::new();
    let mut names: Vec<&str> = Vec::new();

    for fields in contracts {
        let contract = Contract::new(fields);
        let label = format!(
            "semantic-owner controller contract {}",
            contract.path().json()
        );
        // Each finding is keyed by its row's path and its check, which stay when a message is
        // reworded.
        let row = contract.path().text();
        let mut report = |check: &str, message: String| report(format!("{row}#{check}"), message);

        unique(contract, &label, &mut paths, &mut names, &mut report);

        if !contract
            .primary_service_name()
            .string()
            .is_some_and(|name| name.ends_with("Service"))
            || contract.primary_service_path().string().is_none()
        {
            report(
                "service",
                format!("{label} must provide a primary Service class name"),
            );
        }

        let classification = contract.classification();

        if ![CUSTOM_USE_CASE, ENTITY_RESOURCE, EXTERNAL_NOUN, PLAIN]
            .iter()
            .any(|known| classification == Js::Str(known))
        {
            report(
                "classification",
                format!(
                    "{label} has unsupported classification {}",
                    classification.json()
                ),
            );
        }

        semantic_owner(contract, &label, &mut report);

        if (contract.own_service_entity_name() != Js::Undefined
            || contract.own_service_entity_path() != Js::Undefined)
            && (classification != Js::Str(CUSTOM_USE_CASE)
                || contract.own_service_entity_name().string().is_none()
                || contract.own_service_entity_path().string().is_none())
        {
            report(
                "service-entity",
                format!(
                    "{label} a distinct primary service entity requires CUSTOM_USE_CASE and both primaryServiceEntityName/primaryServiceEntityPath"
                ),
            );
        }

        let invalid_entity = if contract.plain() {
            contract.entity_name() != Js::Null || contract.entity_path() != Js::Null
        } else {
            contract.entity_name().string().is_none() || contract.entity_path().string().is_none()
        };

        if invalid_entity {
            report(
                "entity",
                format!(
                    "{label} has an invalid entityName for {}",
                    classification.text()
                ),
            );
        }

        if (classification == Js::Str(CUSTOM_USE_CASE) || classification == Js::Str(EXTERNAL_NOUN))
            && !decision(contract.decision_id())
        {
            report(
                "decision",
                format!(
                    "{label} requires an exact decisionId for {}",
                    classification.text()
                ),
            );
        }

        if classification == Js::Str(EXTERNAL_NOUN)
            && !contract
                .path()
                .string()
                .is_some_and(|path| path.starts_with(external))
        {
            report(
                "external",
                format!(
                    "{label} classifies an external noun outside {}",
                    external.trim_end_matches('/')
                ),
            );
        }
    }
}

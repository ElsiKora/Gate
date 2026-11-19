//! Contextual advisory-lock resources: static resources whose identifier is too small to stand for
//! the resource in source text, such as `1`. The `contextual-resources` option names each one by its
//! family and resource with its identifier, and their identifiers leave the registered-identifier
//! inventory, while their family identifiers stay in it. Which resources are contextual is a
//! reviewed choice, not bounded by the rule. The list must match the manifest exactly, or the
//! inventory keeps every identifier.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::Value;

/// One contextual resource.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct ContextualResource {
    /// The name of the resource's family.
    pub group: String,
    /// The name of the resource.
    pub name: String,
    /// The resource identifier the manifest gives it.
    pub id: i64,
}

/// The static resources of the manifest: family name, resource name and identifier.
fn resources(manifest: &Value) -> Vec<(&str, &str, Option<i64>)> {
    manifest
        .get("families")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .flat_map(|family| {
            let group = family
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();

            family
                .get("resources")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .map(move |resource| {
                    (
                        group,
                        resource
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        resource.get("resourceId").and_then(Value::as_i64),
                    )
                })
        })
        .collect()
}

/// The resource identifiers the listed contextual resources take out of the inventory, each once.
///
/// # Errors
/// Returns why the list does not match the manifest: a resource listed twice, a resource the
/// manifest lacks or gives another identifier, or a manifest resource with a listed identifier
/// that the list does not name.
pub fn excluded(manifest: &Value, listed: &[ContextualResource]) -> Result<Vec<i64>, String> {
    let resources = resources(manifest);

    for (index, entry) in listed.iter().enumerate() {
        let (group, name) = (entry.group.as_str(), entry.name.as_str());

        if listed[..index]
            .iter()
            .any(|earlier| earlier.group == group && earlier.name == name)
        {
            return Err(format!(
                "Contextual resource {group}.{name} is listed twice."
            ));
        }

        match resources
            .iter()
            .find(|(found_group, found_name, _)| *found_group == group && *found_name == name)
        {
            None => {
                return Err(format!(
                    "Contextual resource {group}.{name} is not a resource of the manifest."
                ));
            }
            Some((_, _, id)) if *id != Some(entry.id) => {
                return Err(format!(
                    "Contextual resource {group}.{name} has the resource identifier {} in the manifest, not {}.",
                    id.map_or_else(|| "none".to_owned(), |id| id.to_string()),
                    entry.id
                ));
            }
            Some(_) => {}
        }
    }

    let mut ids: Vec<i64> = listed.iter().map(|entry| entry.id).collect();
    ids.sort_unstable();
    ids.dedup();

    for (group, name, id) in &resources {
        let shares = id.is_some_and(|id| ids.contains(&id));
        let named = listed
            .iter()
            .any(|entry| entry.group == *group && entry.name == *name);

        if shares && !named {
            return Err(format!(
                "Manifest resource {group}.{name} has a contextual resource identifier but is not listed as contextual."
            ));
        }
    }

    Ok(ids)
}

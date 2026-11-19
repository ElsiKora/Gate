//! `nestjs/advisory-lock-manifest`: the database advisory-lock registry manifest is valid, the
//! operation enum holds exactly the advisory-lock operations, and every static-key marker in the
//! source names a manifest resource with its exact identifiers.
//!
//! The manifest lists families with their signed int32 identifiers, scope, rank, predecessors and
//! either admitted operations (a dynamic family) or static resources with theirs. A
//! `<marker-token> <FAMILY>.<RESOURCE>=<family id>,<resource id>` marker in a source line points at
//! a static resource; `marker-paths` names the exact files of a marker and `required-markers` the
//! markers that must be present.

use crate::shared::paths::{self, Form};
use std::collections::{BTreeMap, BTreeSet, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::{self, Map, Value};
use gate_sdk::{Rule, Run};
use gate_typescript::owners::{property_access, top_level};
use gate_typescript::oxc_ast::ast::{
    Argument, ChainElement, Declaration, Expression, Statement, TSEnumMemberName,
};
use gate_typescript::{Allocator, SourceFile, TypeScript};
use regex_automata::meta::Regex;

use crate::layout::{Layout, PluginOptions};
use crate::shared::pattern;
use crate::shared::project;
use crate::shared::report::Findings;
use crate::shared::tree;

const FAMILY_KEYS: [&str; 8] = [
    "name",
    "familyId",
    "scope",
    "resourceKind",
    "rank",
    "predecessors",
    "allowedOperations",
    "resources",
];
const RESOURCE_KEYS: [&str; 3] = ["name", "resourceId", "allowedOperations"];

/// The options of `nestjs/advisory-lock-manifest`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The registry manifest; `common/advisory-lock/registry.manifest.json` below the source root
    /// when not set.
    pub manifest_path: Option<String>,
    /// The operation enum's file; `common/advisory-lock/enum/operation.enum.ts` below the source
    /// root when not set.
    pub operation_enum_path: Option<String>,
    /// The operation enum's name.
    pub operation_enum_name: String,
    /// The advisory-lock operations, in enum order.
    #[schemars(length(min = 1))]
    pub operations: Vec<String>,
    /// The token that starts a static-key marker.
    pub marker_token: String,
    /// The exact files of a marker, sorted, by `FAMILY.RESOURCE`.
    pub marker_paths: BTreeMap<String, Vec<String>>,
    /// The markers that must be present.
    pub required_markers: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            manifest_path: None,
            operation_enum_path: None,
            operation_enum_name: "EDatabaseAdvisoryLockOperation".to_owned(),
            operations: [
                "SESSION_LOCK_EXCLUSIVE",
                "SESSION_LOCK_SHARED",
                "SESSION_TRY_LOCK_EXCLUSIVE",
                "SESSION_TRY_LOCK_SHARED",
                "SESSION_UNLOCK_EXCLUSIVE",
                "SESSION_UNLOCK_SHARED",
                "TRANSACTION_LOCK_EXCLUSIVE",
                "TRANSACTION_LOCK_SHARED",
                "TRANSACTION_TRY_LOCK_EXCLUSIVE",
                "TRANSACTION_TRY_LOCK_SHARED",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            marker_token: "DATABASE_ADVISORY_LOCK_STATIC_KEY".to_owned(),
            marker_paths: BTreeMap::new(),
            required_markers: Vec::new(),
        }
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct AdvisoryLockManifest;

impl Options {
    /// The registry manifest, set or derived from the source root.
    fn manifest_path(&self, layout: &Layout) -> String {
        self.manifest_path.clone().unwrap_or_else(|| {
            format!(
                "{}common/advisory-lock/registry.manifest.json",
                layout.source_prefix()
            )
        })
    }

    /// The operation enum's file, set or derived from the source root.
    fn operation_enum_path(&self, layout: &Layout) -> String {
        self.operation_enum_path.clone().unwrap_or_else(|| {
            format!(
                "{}common/advisory-lock/enum/operation.enum.ts",
                layout.source_prefix()
            )
        })
    }

    /// Checks that every path option is a normalized path relative to the project root, and that
    /// `operation-enum-path` names a JavaScript or TypeScript file.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if let Some(manifest) = &self.manifest_path {
            paths::check("manifest-path", manifest, Form::File)?;
        }
        if let Some(operation_enum) = &self.operation_enum_path {
            paths::check("operation-enum-path", operation_enum, Form::File)?;
            gate_typescript::check_source_option("operation-enum-path", operation_enum)?;
        }
        for (marker, files) in &self.marker_paths {
            paths::check_all(&format!("marker-paths.{marker}"), files, Form::File)?;
        }
        Ok(())
    }
}

impl Rule for AdvisoryLockManifest {
    type Options = Options;

    const NAME: &'static str = "advisory-lock-manifest";
    const TITLE: &'static str =
        "The advisory-lock manifest, operation enum and static-key markers agree";
    const ASPECTS: &'static [&'static str] = &["manifest", "marker", "operation-enum"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        run.options().check_paths()?;
        let options = run.options();
        let manifest_path = options.manifest_path(&layout);
        let operation_enum_path = options.operation_enum_path(&layout);
        let mut findings = Findings::default();

        // An enabled rule needs the registry: without it, the configuration is wrong.
        let Some(text) = run.tree().text(&manifest_path) else {
            findings.push_keyed(
                "manifest",
                manifest_path.as_str(),
                None,
                "untracked",
                format!(
                    "The advisory-lock manifest {manifest_path} is not tracked; track it, set `manifest-path` to the manifest, or leave the rule off."
                ),
            );
            findings.report(run, help);
            return Ok(());
        };

        let marker = pattern::compile(
            "marker-token",
            &format!(
                "{} ([A-Z][A-Z0-9_]*)\\.([A-Z][A-Z0-9_]*)=(-?[0-9]+),(-?[0-9]+)",
                pattern::escape(&options.marker_token)
            ),
        )?;
        let operations: Vec<&str> = options.operations.iter().map(String::as_str).collect();

        // The manifest's exact formatted form has LF line ends.
        if text.contains('\r') {
            findings.push(
                "manifest",
                manifest_path.as_str(),
                None,
                "Manifest is not in its exact formatted form: it has a carriage return.",
            );
        }

        let static_keys = match serde_json::from_str::<Value>(&text) {
            Ok(manifest) => {
                let messages = manifest_findings(&manifest, &operations);
                let valid = messages.is_empty();

                for message in messages {
                    findings.push("manifest", manifest_path.as_str(), None, message);
                }

                valid.then(|| static_keys(&manifest))
            }
            Err(error) => {
                findings.push_keyed(
                    "manifest",
                    manifest_path.as_str(),
                    None,
                    "json",
                    format!("Manifest is not valid JSON: {error}"),
                );
                None
            }
        };

        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);

        operation_enum(
            &typescript,
            &operation_enum_path,
            &options.operation_enum_name,
            &operations,
            &mut findings,
        )?;

        // Markers are compared with a valid manifest only.
        if let Some(static_keys) = static_keys {
            let files: Vec<String> = tree::files(&source, &layout.source_root, &tree::UNSKIPPED)
                // Every file whose name ends in `.ts`, in that exact case, in every folder.
                .filter(|path| path.strip_suffix(".ts").is_some())
                .map(str::to_owned)
                .collect();
            let texts: Vec<(String, String)> = files
                .into_iter()
                .filter_map(|path| {
                    let text = run.tree().text(&path)?;
                    Some((path, text))
                })
                .collect();

            markers(
                &Markers {
                    token: &options.marker_token,
                    pattern: &marker,
                    static_keys: &static_keys,
                    paths: &options.marker_paths,
                    required: &options.required_markers,
                    manifest: &manifest_path,
                },
                &texts,
                &mut findings,
            );
        }

        findings.report(run, help);
        Ok(())
    }
}

/// Reports the operation enum when it is missing or does not list `operations`, in order.
fn operation_enum(
    typescript: &TypeScript<'_>,
    path: &str,
    name: &str,
    operations: &[&str],
    findings: &mut Findings,
) -> gate_sdk::Result<()> {
    match typescript.parse(path)? {
        None => findings.push(
            "operation-enum",
            path,
            None,
            "The operation enum is missing.",
        ),
        // A file without a syntax tree is reported by `typescript/parse`.
        Some(file) if file.fatal() => {}
        Some(file) => {
            if let Some(message) = operation_enum_finding(&file, name, operations) {
                findings.push_keyed("operation-enum", path, None, "declaration", message);
            }
        }
    }

    Ok(())
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "manifest" => "correct the manifest, then regenerate the registry from it",
        "operation-enum" => {
            "declare exactly the advisory-lock operations in the operation enum and freeze it once"
        }
        _ => "write each static-key marker with its manifest identifiers, in its listed files",
    }
}

/// The findings of a manifest: its header, each family and resource, and the predecessors.
fn manifest_findings(manifest: &Value, known: &[&str]) -> Vec<String> {
    let Some(object) = exact_keys(manifest, &["schemaVersion", "resourceDomain", "families"])
    else {
        return vec!["Manifest has an invalid shape.".to_owned()];
    };
    let families = object.get("families").and_then(Value::as_array);
    let header = object.get("schemaVersion").and_then(Value::as_f64) == Some(1.0)
        && object
            .get("resourceDomain")
            .and_then(Value::as_str)
            .is_some_and(|domain| !domain.is_empty())
        && families.is_some_and(|families| !families.is_empty());
    let (true, Some(families)) = (header, families) else {
        return vec!["Manifest header is invalid.".to_owned()];
    };

    let mut seen = Seen::default();
    let mut findings: Vec<String> = families
        .iter()
        .filter_map(|family| family_finding(family, known, &mut seen).err())
        .collect();

    // Predecessors are compared once every family is valid.
    if findings.is_empty() {
        findings.extend(predecessor_findings(families));
    }

    findings
}

/// The family names, family identifiers and resource identifiers seen so far.
#[derive(Default)]
struct Seen {
    families: HashSet<String>,
    family_ids: HashSet<i64>,
    resource_ids: HashSet<i64>,
}

/// The first finding of one family and its resources.
fn family_finding(family: &Value, known: &[&str], seen: &mut Seen) -> Result<(), String> {
    let object = exact_keys(family, &FAMILY_KEYS).ok_or("Family has an invalid shape.")?;
    let name = registry_name(object.get("name"))
        .ok_or("Family name must be an upper-case registry name.")?;
    let family_id = int32(object.get("familyId"))
        .ok_or_else(|| format!("Family {name} ID must be a signed int32."))?;

    if seen.families.contains(name) || seen.family_ids.contains(&family_id) {
        return Err(format!("Family {name} is duplicated."));
    }

    seen.families.insert(name.to_owned());
    seen.family_ids.insert(family_id);

    let scope = object.get("scope").and_then(Value::as_str);
    let kind = object.get("resourceKind").and_then(Value::as_str);
    let rank = object.get("rank");
    let predecessors = object.get("predecessors").and_then(Value::as_array);
    let resources = object.get("resources").and_then(Value::as_array);
    let ranked = match scope {
        Some("PRODUCTION") => integer(rank).is_some_and(|rank| rank > 0.0),
        Some("NON_PRODUCTION") => rank.is_some_and(Value::is_null),
        _ => false,
    };

    let (true, Some(predecessors), Some(resources)) = (
        ranked
            && matches!(kind, Some("DYNAMIC" | "STATIC"))
            && predecessors.is_some_and(|items| unique(items)),
        predecessors,
        resources,
    ) else {
        return Err(format!("Family {name} metadata is invalid."));
    };

    if predecessors
        .iter()
        .any(|predecessor| registry_name(Some(predecessor)).is_none())
    {
        return Err("Predecessor must be an upper-case registry name.".to_owned());
    }

    let dynamic = kind == Some("DYNAMIC");

    if !operations(object.get("allowedOperations"), dynamic, known) {
        return Err(format!("Family {name} has invalid operations."));
    }

    if dynamic && !resources.is_empty() {
        return Err(format!(
            "Dynamic family {name} cannot own static resources."
        ));
    }

    let admits = object
        .get("allowedOperations")
        .and_then(Value::as_array)
        .is_some_and(|operations| !operations.is_empty());

    if !dynamic && (admits || resources.is_empty()) {
        return Err(format!(
            "Static family {name} violates the operation/resource XOR."
        ));
    }

    let mut local = HashSet::new();

    for resource in resources {
        let object =
            exact_keys(resource, &RESOURCE_KEYS).ok_or("Resource has an invalid shape.")?;
        let resource_name = registry_name(object.get("name"))
            .ok_or("Resource name must be an upper-case registry name.")?;
        let resource_id = int32(object.get("resourceId"))
            .ok_or_else(|| format!("Resource {name}.{resource_name} ID must be a signed int32."))?;

        if !operations(object.get("allowedOperations"), true, known) {
            return Err(format!(
                "Resource {name}.{resource_name} has invalid operations."
            ));
        }

        if !local.insert(resource_name) || !seen.resource_ids.insert(resource_id) {
            return Err(format!("Resource {name}.{resource_name} is duplicated."));
        }
    }

    Ok(())
}

/// The findings of predecessors that are unknown, of another scope or not of a lower rank.
fn predecessor_findings(families: &[Value]) -> Vec<String> {
    let by_name: BTreeMap<&str, &Value> = families
        .iter()
        .filter_map(|family| Some((family.get("name")?.as_str()?, family)))
        .collect();
    let mut findings = Vec::new();

    for family in families {
        let name = family
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let rank = integer(family.get("rank"));

        for predecessor in family
            .get("predecessors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let predecessor_name = predecessor.as_str().unwrap_or_default();
            let valid = by_name.get(predecessor_name).is_some_and(|candidate| {
                candidate.get("scope") == family.get("scope")
                    && integer(candidate.get("rank"))
                        .zip(rank)
                        .is_some_and(|(before, after)| before < after)
            });

            if !valid {
                findings.push(format!(
                    "Family {name} has an invalid predecessor {predecessor_name}."
                ));
            }
        }
    }

    findings
}

/// The object when `value` is an object with exactly `keys` in that order.
fn exact_keys<'v>(value: &'v Value, keys: &[&str]) -> Option<&'v Map<String, Value>> {
    value
        .as_object()
        .filter(|object| object.keys().map(String::as_str).eq(keys.iter().copied()))
}

/// The name when `value` is an upper-case registry name.
fn registry_name(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|name| {
        let mut characters = name.chars();
        characters
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
            && characters.all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
    })
}

/// The number when `value` is an integer, as JavaScript's `Number.isInteger` reads it.
fn integer(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && number.fract() == 0.0)
}

/// The integer when `value` is a signed int32.
fn int32(value: Option<&Value>) -> Option<i64> {
    integer(value)
        .filter(|number| (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(number))
        .map(|number| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the value is an integer within the int32 range"
            )]
            let number = number as i64;
            number
        })
}

/// Whether the items of an array are pairwise distinct.
fn unique(items: &[Value]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(index, item)| !items[..index].contains(item))
}

/// Whether `value` is a list of distinct `known` operations, non-empty when `nonempty`.
fn operations(value: Option<&Value>, nonempty: bool, known: &[&str]) -> bool {
    value.and_then(Value::as_array).is_some_and(|items| {
        (!nonempty || !items.is_empty())
            && unique(items)
            && items.iter().all(|item| {
                item.as_str()
                    .is_some_and(|operation| known.contains(&operation))
            })
    })
}

/// The family and resource identifiers of every static resource, by `<FAMILY>.<RESOURCE>`.
fn static_keys(manifest: &Value) -> BTreeMap<String, (i64, i64)> {
    let mut keys = BTreeMap::new();

    for family in manifest
        .get("families")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(name), Some(family_id)) = (
            family.get("name").and_then(Value::as_str),
            int32(family.get("familyId")),
        ) else {
            continue;
        };

        for resource in family
            .get("resources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(resource_name), Some(resource_id)) = (
                resource.get("name").and_then(Value::as_str),
                int32(resource.get("resourceId")),
            ) {
                keys.insert(format!("{name}.{resource_name}"), (family_id, resource_id));
            }
        }
    }

    keys
}

/// The finding of an operation enum source, if any: it must be exactly the exported enum `name` of
/// `operations`, each its own value, frozen once.
fn operation_enum_finding(
    source: &SourceFile<'_>,
    name: &str,
    operations: &[&str],
) -> Option<String> {
    let exact = format!(
        "Operation enum does not contain the exact {} operations.",
        count(operations.len())
    );
    let statements: Vec<&Statement<'_>> = top_level(source).collect();
    let declaration = match statements.first() {
        Some(Statement::ExportDeclaration(export)) => match &export.declaration {
            Declaration::TSEnumDeclaration(declaration) => Some(declaration),
            _ => None,
        },
        _ => None,
    };
    // A directive such as `"use strict"` is a statement to the TypeScript compiler, not to oxc.
    let Some(declaration) = declaration.filter(|declaration| {
        statements.len() == 2
            && source.program().directives.is_empty()
            && declaration.id.name == name
            && !declaration.r#const
            && !declaration.declare
            && declaration.body.members.len() == operations.len()
    }) else {
        return Some(exact);
    };

    let frozen = match statements.get(1) {
        Some(Statement::ExpressionStatement(statement)) => {
            // An optional call is a call to the TypeScript compiler, and a chain to oxc.
            let call = match &statement.expression {
                Expression::CallExpression(call) => Some(&**call),
                Expression::ChainExpression(chain) => match &chain.expression {
                    ChainElement::CallExpression(call) => Some(&**call),
                    _ => None,
                },
                _ => None,
            };

            call.is_some_and(|call| {
                property_access(&call.callee).is_some_and(|(object, name)| {
                    name == "freeze"
                        && matches!(object, Expression::Identifier(identifier) if identifier.name == "Object")
                }) && matches!(
                    call.arguments.as_slice(),
                    [Argument::Identifier(argument)] if argument.name == name
                )
            })
        }
        _ => false,
    };

    if !frozen {
        return Some("Operation enum object is not frozen exactly once.".to_owned());
    }

    let mut entries = Vec::new();

    for member in &declaration.body.members {
        let (TSEnumMemberName::Identifier(name), Some(Expression::StringLiteral(value))) =
            (&member.id, &member.initializer)
        else {
            return Some(
                "Operation enum members must have unquoted names and string initializers."
                    .to_owned(),
            );
        };
        entries.push((name.name.as_str(), value.value.as_str()));
    }

    let matches = entries
        .iter()
        .zip(operations)
        .all(|(&(member, value), operation)| member == *operation && value == *operation);

    (!matches).then_some(exact)
}

/// A count as the enum message words it: `ten` for the ten advisory-lock operations.
fn count(number: usize) -> String {
    const WORDS: [&str; 11] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];

    WORDS
        .get(number)
        .map_or_else(|| number.to_string(), |word| (*word).to_owned())
}

/// What the marker check compares the markers with.
struct Markers<'o> {
    token: &'o str,
    pattern: &'o Regex,
    static_keys: &'o BTreeMap<String, (i64, i64)>,
    paths: &'o BTreeMap<String, Vec<String>>,
    required: &'o [String],
    manifest: &'o str,
}

/// Reports static-key markers that are malformed, name no manifest resource with its identifiers,
/// stand outside their exact files or more than once, and required markers that are absent.
fn markers(markers: &Markers<'_>, texts: &[(String, String)], findings: &mut Findings) {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut wrong = false;
    let mut captures = markers.pattern.create_captures();

    for (path, text) in texts {
        for (index, line) in text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(markers.token))
        {
            let number = u32::try_from(index + 1).ok();
            markers.pattern.captures(line, &mut captures);

            if !captures.is_match() {
                findings.push_keyed(
                    "marker",
                    path.as_str(),
                    number,
                    "malformed",
                    format!("Malformed static-key marker in {path}."),
                );
                wrong = true;
                continue;
            }

            let group = |index: usize| captures.get_group(index).map_or("", |span| &line[span]);
            let name = format!("{}.{}", group(1), group(2));
            let identifiers = group(3)
                .parse::<i64>()
                .ok()
                .zip(group(4).parse::<i64>().ok());

            if markers.static_keys.get(&name).copied() != identifiers || identifiers.is_none() {
                findings.push(
                    "marker",
                    path.as_str(),
                    number,
                    format!("Static-key marker {name} does not match the manifest."),
                );
                wrong = true;
                continue;
            }

            found.entry(name).or_default().push(path.clone());
        }
    }

    // Where a marker line is wrong, its places and presence would only repeat that finding.
    if wrong {
        return;
    }

    for (name, paths) in &found {
        let mut sorted = paths.clone();
        sorted.sort();

        match markers.paths.get(name) {
            Some(expected) if sorted != *expected => findings.push(
                "marker",
                sorted[0].as_str(),
                None,
                format!("Static-key marker {name} does not match its exact migration paths."),
            ),
            None if paths.len() > 1 => findings.push(
                "marker",
                sorted[1].as_str(),
                None,
                format!(
                    "Static-key marker {name} is duplicated outside its exact migration paths."
                ),
            ),
            _ => {}
        }
    }

    let present: BTreeSet<&str> = found.keys().map(String::as_str).collect();

    for marker in markers
        .required
        .iter()
        .filter(|marker| !present.contains(marker.as_str()))
    {
        findings.push(
            "marker",
            markers.manifest,
            None,
            format!("Required static-key marker {marker} is absent."),
        );
    }
}

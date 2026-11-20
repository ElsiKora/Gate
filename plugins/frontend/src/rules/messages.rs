//! `frontend/messages`: every locale has the same message keys, no message is empty, and every
//! literal key the code passes to its translation function is a message of the base locale.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::Value;
use gate_sdk::{Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, BindingPattern, CallExpression, Expression, ObjectExpression, ObjectPropertyKind,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::value::string_like;

use crate::code::Project;
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{FileName, folder_of, name_of};

/// The rule.
#[derive(Debug, Default)]
pub struct Messages;

/// The options of `frontend/messages`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The locale whose messages every other locale and every key in the code is checked against.
    pub base_locale: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_locale: "en".to_owned(),
        }
    }
}

impl Rule<TypeScriptCall> for Messages {
    type Options = Options;

    const NAME: &'static str = "messages";
    const TITLE: &'static str = "Every locale has the same messages, none empty, and the code uses only messages that exist";
    const ASPECTS: &'static [&'static str] = &["locale-keys", "empty", "unknown-key"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project, &run.options().base_locale)?;
            report(run, found);
            Ok(())
        })
    }
}

/// One locale file: its path and its message keys, dotted.
struct Locale {
    path: String,
    keys: BTreeSet<String>,
}

fn check(project: &Project<'_, '_>, base_locale: &str) -> gate_sdk::Result<Vec<Found>> {
    let folder = project.pack.messages_folder();
    let base_path = format!("{folder}/{base_locale}.json");
    let mut found = Vec::new();
    let mut locales = Vec::new();

    for path in &project.paths {
        if folder_of(path) != folder || FileName::new(name_of(path)).extension != "json" {
            continue;
        }
        let Some(text) = project.text(path)? else {
            continue;
        };
        let value: Value = gate_sdk::serde_json::from_str(&text).map_err(|error| {
            gate_sdk::Error::new(format!(
                "{path} is not JSON ({error}); fix the messages file, which frontend/messages reads"
            ))
        })?;
        let mut keys = BTreeSet::new();
        leaves(&value, "", &mut |key, leaf| {
            if leaf.as_str().is_some_and(|text| text.trim().is_empty()) {
                found.push(
                    Found::at(
                        "empty",
                        path,
                        format!("{path} has the empty message {key}"),
                        "write the message, or remove the key from every locale",
                    )
                    .named(key),
                );
            }
            keys.insert(key.to_owned());
        });
        locales.push(Locale {
            path: (*path).to_owned(),
            keys,
        });
    }

    if locales.is_empty() {
        return Ok(found);
    }

    let Some(base) = locales.iter().find(|locale| locale.path == base_path) else {
        found.push(Found::at(
            "locale-keys",
            folder,
            format!("{folder}/ has no {base_locale}.json, the base locale"),
            "add the base locale's messages, or set base-locale to the locale the app is written in",
        ));
        return Ok(found);
    };

    for locale in &locales {
        if locale.path == base.path {
            continue;
        }
        for key in locale.keys.difference(&base.keys) {
            found.push(
                Found::at(
                    "locale-keys",
                    &locale.path,
                    format!(
                        "{} has the message {key}, which {} lacks",
                        locale.path, base.path
                    ),
                    "add the message to every locale, or remove it from this one",
                )
                .named(key),
            );
        }
        for key in base.keys.difference(&locale.keys) {
            found.push(
                Found::at(
                    "locale-keys",
                    &locale.path,
                    format!("{} lacks the message {key} of {}", locale.path, base.path),
                    "translate the message into this locale",
                )
                .named(key),
            );
        }
    }

    unknown_keys(project, base, &mut found)?;
    Ok(found)
}

/// Reports every literal key the code shows that `base` lacks.
fn unknown_keys(
    project: &Project<'_, '_>,
    base: &Locale,
    found: &mut Vec<Found>,
) -> gate_sdk::Result<()> {
    for path in project.all_code() {
        let Some(file) = project.parse(path)? else {
            continue;
        };
        for (key, line) in used_keys(project.pack, &file) {
            if !base.keys.contains(&key) {
                found.push(
                    Found::at(
                        "unknown-key",
                        path,
                        format!("{path} shows the message {key}, which {} lacks", base.path),
                        "add the message to every locale, or correct the key",
                    )
                    .line(line)
                    .named(&key),
                );
            }
        }
    }
    Ok(())
}

/// Calls `leaf` with the dotted key of every leaf of `value`.
fn leaves(value: &Value, prefix: &str, leaf: &mut dyn FnMut(&str, &Value)) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let dotted = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                leaves(child, &dotted, leaf);
            }
        }
        _ if !prefix.is_empty() => leaf(prefix, value),
        _ => {}
    }
}

/// The literal keys `file` passes to its translation functions, as the pack binds them, each with
/// its namespace or key prefix, and the line of the call.
fn used_keys(pack: Pack, file: &SourceFile<'_>) -> Vec<(String, u32)> {
    let bound = bound_functions(pack, file);
    let semantic = file.semantic();
    let nodes = semantic.nodes();
    let mut keys = Vec::new();

    for (symbol, prefix) in bound {
        for reference in semantic.scoping().get_resolved_references(symbol) {
            let node = nodes.get_node(reference.node_id());
            let call = match nodes.parent_kind(node.id()) {
                AstKind::CallExpression(call) if call.callee.span() == node.span() => call,
                AstKind::StaticMemberExpression(member)
                    if matches!(member.property.name.as_str(), "rich" | "markup" | "raw") =>
                {
                    let member_id = nodes.parent_id(node.id());
                    match nodes.parent_kind(member_id) {
                        AstKind::CallExpression(call) if call.callee.span() == member.span => call,
                        _ => continue,
                    }
                }
                _ => continue,
            };
            let Some(key) = first_text(call) else {
                continue;
            };
            let full = if prefix.is_empty() {
                key
            } else {
                format!("{prefix}.{key}")
            };
            keys.push((full, file.line(call.span.start)));
        }
    }

    keys
}

/// The translation functions `file` binds, each with its namespace or key prefix: next-intl's
/// `useTranslations(ns)` and `getTranslations(ns)` in Next.js, react-i18next's
/// `const { t } = useTranslation(ns, { keyPrefix })` elsewhere.
fn bound_functions(pack: Pack, file: &SourceFile<'_>) -> Vec<(SymbolId, String)> {
    let factories: &[(&str, &str)] = if pack == Pack::Nextjs {
        &[
            ("next-intl", "useTranslations"),
            ("next-intl/server", "getTranslations"),
        ]
    } else {
        &[("react-i18next", "useTranslation")]
    };
    let mut bound = Vec::new();

    for (package, export) in factories {
        let Some(bindings) = ImportBindings::new(file, package, false) else {
            continue;
        };
        for node in file.semantic().nodes().iter() {
            let AstKind::VariableDeclarator(declarator) = node.kind() else {
                continue;
            };
            let Some(init) = &declarator.init else {
                continue;
            };
            let init = match init {
                Expression::AwaitExpression(awaited) => &awaited.argument,
                other => other,
            };
            let Expression::CallExpression(call) = init else {
                continue;
            };
            if bindings
                .resolve(&call.callee)
                .is_none_or(|names| names != [*export])
            {
                continue;
            }

            if pack == Pack::Nextjs {
                if let BindingPattern::BindingIdentifier(identifier) = &declarator.id
                    && let Some(symbol) = identifier.symbol_id.get()
                {
                    bound.push((symbol, namespace(call)));
                }
            } else if let BindingPattern::ObjectPattern(object) = &declarator.id {
                for property in &object.properties {
                    if property.key.static_name().as_deref() == Some("t")
                        && let BindingPattern::BindingIdentifier(identifier) = &property.value
                        && let Some(symbol) = identifier.symbol_id.get()
                    {
                        bound.push((symbol, key_prefix(call)));
                    }
                }
            }
        }
    }

    bound
}

/// The first argument of `call` as text, when it is a literal.
fn first_text(call: &CallExpression<'_>) -> Option<String> {
    call.arguments
        .first()
        .and_then(Argument::as_expression)
        .and_then(string_like)
}

/// next-intl's namespace: the first argument, or its `namespace` property.
fn namespace(call: &CallExpression<'_>) -> String {
    if let Some(text) = first_text(call) {
        return text;
    }
    match call.arguments.first().and_then(Argument::as_expression) {
        Some(Expression::ObjectExpression(object)) => property_text(object, "namespace"),
        _ => String::new(),
    }
}

/// react-i18next's key prefix: the `keyPrefix` of the second argument.
fn key_prefix(call: &CallExpression<'_>) -> String {
    match call.arguments.get(1).and_then(Argument::as_expression) {
        Some(Expression::ObjectExpression(object)) => property_text(object, "keyPrefix"),
        _ => String::new(),
    }
}

/// The literal text of the property `name` of `object`; empty when it has none.
fn property_text(object: &ObjectExpression<'_>, name: &str) -> String {
    object
        .properties
        .iter()
        .find_map(|property| match property {
            ObjectPropertyKind::ObjectProperty(property)
                if property.key.static_name().as_deref() == Some(name) =>
            {
                string_like(&property.value)
            }
            _ => None,
        })
        .unwrap_or_default()
}

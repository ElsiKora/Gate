//! `cdk/app`: `cdk.json` runs the standard command and holds only feature flags; `package.json`
//! pins the CDK packages exactly and has the standard scripts.

use gate_sdk::serde_json::{Map, Value};
use gate_sdk::{NoOptions, Rule, Run};

use crate::options::PluginOptions;
use crate::rules::{Found, report};
use crate::sources::Cdk;
use crate::standard::{APP_COMMAND, PINNED_PACKAGES, SCRIPTS};

/// The rule.
#[derive(Debug, Default)]
pub struct App;

impl Rule<Cdk> for App {
    type Options = NoOptions;

    const NAME: &'static str = "app";
    const TITLE: &'static str = "cdk.json runs the standard command with feature flags only, and package.json pins the CDK packages";
    const ASPECTS: &'static [&'static str] = &["command", "context", "pin", "scripts"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        let tree = run.tree();
        let mut found = Vec::new();

        if let Some(text) = tree.text("cdk.json") {
            cdk_json(&text, &mut found);
        }
        if let Some(text) = tree.text("package.json") {
            package_json(&text, &mut found);
        }

        for (aspect, path, key, message, help) in found {
            report(
                run,
                Found {
                    aspect,
                    path,
                    line: None,
                    key,
                    message,
                    help,
                },
            );
        }

        Ok(())
    }
}

/// aspect, path, key, message, help
type Entry = (&'static str, &'static str, String, String, &'static str);

fn cdk_json(text: &str, found: &mut Vec<Entry>) {
    let Ok(Value::Object(document)) = gate_sdk::serde_json::from_str::<Value>(text) else {
        found.push((
            "command",
            "cdk.json",
            "unreadable".to_owned(),
            "cdk.json is not a JSON object".to_owned(),
            "make cdk.json a JSON object with app and context",
        ));
        return;
    };

    if document.get("app").and_then(Value::as_str) != Some(APP_COMMAND) {
        found.push((
            "command",
            "cdk.json",
            "app".to_owned(),
            format!("cdk.json's app is not the standard command `{APP_COMMAND}`"),
            "set app to the standard command",
        ));
    }

    if let Some(Value::Object(context)) = document.get("context") {
        for key in context.keys().filter(|key| !key.contains(':')) {
            found.push((
                "context",
                "cdk.json",
                (*key).clone(),
                format!(
                    "cdk.json's context holds `{key}`, which is not a CDK feature flag; project \
                     values come from the configuration file"
                ),
                "move the value into the configuration file and read it from there",
            ));
        }
    }
}

fn package_json(text: &str, found: &mut Vec<Entry>) {
    let Ok(Value::Object(document)) = gate_sdk::serde_json::from_str::<Value>(text) else {
        found.push((
            "pin",
            "package.json",
            "unreadable".to_owned(),
            "package.json is not a JSON object".to_owned(),
            "make package.json a JSON object",
        ));
        return;
    };
    let empty = Map::new();
    let object = |name: &str| section(&document, &empty, name);

    for package in PINNED_PACKAGES {
        let version = object("dependencies")
            .get(package)
            .or_else(|| object("devDependencies").get(package))
            .and_then(Value::as_str);
        match version {
            None => found.push((
                "pin",
                "package.json",
                package.to_owned(),
                format!("package.json does not depend on {package}"),
                "add the package with an exact version",
            )),
            Some(version) if !is_exact(version) => found.push((
                "pin",
                "package.json",
                package.to_owned(),
                format!("package.json depends on {package} as `{version}`, which is not an exact version"),
                "pin the exact version, such as 2.180.0, or a workspace catalog entry (catalog:)",
            )),
            Some(_) => {}
        }
    }

    for script in SCRIPTS {
        if !object("scripts").contains_key(script) {
            found.push((
                "scripts",
                "package.json",
                script.to_owned(),
                format!("package.json has no `{script}` script"),
                "add the script",
            ));
        }
    }
}

/// The object `name` of `document`, or `empty`.
fn section<'d>(
    document: &'d Map<String, Value>,
    empty: &'d Map<String, Value>,
    name: &str,
) -> &'d Map<String, Value> {
    match document.get(name) {
        Some(Value::Object(section)) => section,
        _ => empty,
    }
}

/// `1.2.3`, or a workspace catalog reference.
fn is_exact(version: &str) -> bool {
    if version.starts_with("catalog:") {
        return true;
    }
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

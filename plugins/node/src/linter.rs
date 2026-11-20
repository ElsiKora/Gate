//! `node/linter`: one flat ESLint configuration at the root, which extends the shared
//! configuration and which every member imports.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Result, Rule, Run, Scope, Tree};

use crate::repo::{self, directory, file_name, join, normalize};
use crate::settings::Settings;
use crate::standard::SHARED_ESLINT;

#[derive(Debug, Default)]
pub(crate) struct Linter;

/// The options of `node/linter`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) struct Options {
    /// The package of the shared ESLint configuration the root configuration extends.
    #[serde(default = "shared")]
    shared: String,
}

fn shared() -> String {
    SHARED_ESLINT.to_owned()
}

/// The one form of the configuration.
const CONFIG: &str = "eslint.config.mjs";

impl Rule for Linter {
    type Options = Options;
    const NAME: &'static str = "linter";
    const TITLE: &'static str = "One flat ESLint configuration at the root, extending the shared one and imported by the members";
    const ASPECTS: &'static [&'static str] = &["legacy", "root", "shared", "detached"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        Settings::read(run)?;
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        let paths = tree.paths();
        let mut uses = false;
        for path in &manifests {
            let Some(manifest) = repo::read_json(&tree, path)? else {
                continue;
            };
            if let Some(member) = manifest.member("eslintConfig") {
                uses = true;
                crate::report(
                    run.report(),
                    "legacy",
                    (path, Some(member.line)),
                    "eslintConfig",
                    format!("{path} configures ESLint in its legacy \"eslintConfig\" key"),
                    "move the configuration to the root eslint.config.mjs and remove the key",
                );
            }
            uses |= ["dependencies", "devDependencies"].iter().any(|section| {
                manifest
                    .get(section)
                    .is_some_and(|deps| deps.member("eslint").is_some())
            });
        }
        for path in &paths {
            let name = file_name(path);
            if is_legacy(name) {
                uses = true;
                crate::report(
                    run.report(),
                    "legacy",
                    (path, None),
                    name,
                    format!(
                        "{path} is a legacy ESLint configuration, which ESLint 9 and later no longer read"
                    ),
                    "write the configuration as the flat eslint.config.mjs and remove this file",
                );
            } else if is_flat(name) {
                uses = true;
                if name != CONFIG {
                    crate::report(
                        run.report(),
                        "root",
                        (path, None),
                        name,
                        format!("{path} is named {name}; the one form is {CONFIG}"),
                        "rename it to eslint.config.mjs",
                    );
                }
                if !directory(path).is_empty() {
                    detached(run, &tree, path);
                }
            }
        }
        if uses {
            root(run, &tree);
        }
        Ok(())
    }
}

fn is_legacy(name: &str) -> bool {
    name == ".eslintrc"
        || name.strip_prefix(".eslintrc.").is_some_and(|extension| {
            matches!(extension, "js" | "cjs" | "mjs" | "json" | "yaml" | "yml")
        })
}

fn is_flat(name: &str) -> bool {
    name.strip_prefix("eslint.config.")
        .is_some_and(|extension| matches!(extension, "js" | "mjs" | "cjs" | "ts" | "mts" | "cts"))
}

/// `root` and `shared`: the root configuration exists and extends the shared one, and no other
/// configuration package.
fn root(run: &mut Run<'_, Options>, tree: &Tree<'_>) {
    let Some(text) = tree.text(CONFIG) else {
        crate::report(
            run.report(),
            "root",
            (CONFIG, None),
            CONFIG,
            format!(
                "{CONFIG} is not tracked at the repository root, and the repository uses ESLint"
            ),
            "add eslint.config.mjs at the root, built on the shared configuration",
        );
        return;
    };
    let shared = run.options().shared.clone();
    let specifiers = specifiers(&text);
    let extends = |specifier: &str| {
        specifier == shared
            || specifier
                .strip_prefix(shared.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    };
    if !specifiers
        .iter()
        .any(|(specifier, _)| extends(specifier.as_str()))
    {
        crate::report(
            run.report(),
            "shared",
            (CONFIG, None),
            &shared,
            format!("{CONFIG} does not import {shared}, the shared ESLint configuration"),
            "import the shared package and build the configuration on it, adding only what the repository needs",
        );
    }
    for (specifier, line) in &specifiers {
        if !extends(specifier.as_str()) && is_config_package(specifier) {
            crate::report(
                run.report(),
                "shared",
                (CONFIG, Some(*line)),
                specifier,
                format!("{CONFIG} imports {specifier} as another base; {shared} is the only one"),
                "remove the import; what the repository needs on top goes into its own blocks",
            );
        }
    }
}

/// Whether `specifier` names an ESLint configuration package.
fn is_config_package(specifier: &str) -> bool {
    let package = match specifier.strip_prefix('@') {
        Some(scoped) => scoped.split('/').nth(1).unwrap_or_default(),
        None => specifier.split('/').next().unwrap_or_default(),
    };
    package == "eslint-config" || package.starts_with("eslint-config-")
}

/// `detached`: a member's configuration imports the root's.
fn detached(run: &mut Run<'_, Options>, tree: &Tree<'_>, path: &str) {
    let text = tree.text(path).unwrap_or_default();
    let place = directory(path);
    let imports_root = specifiers(&text).iter().any(|(specifier, _)| {
        specifier.starts_with('.') && normalize(&join(place, specifier)).as_deref() == Some(CONFIG)
    });
    if !imports_root {
        crate::report(
            run.report(),
            "detached",
            (path, None),
            file_name(path),
            format!("{path} does not import the root {CONFIG}, so it can drift from it"),
            "import the root configuration by its relative path and add only what this directory needs",
        );
    }
}

/// The module specifiers of `text`'s static imports, dynamic imports and `require` calls, with
/// their lines. Comments are not skipped: a commented-out import counts.
fn specifiers(text: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for keyword in ["from", "import", "require"] {
            let mut rest = line;
            while let Some(at) = rest.find(keyword) {
                let before = rest[..at].chars().next_back();
                rest = &rest[at + keyword.len()..];
                if before.is_some_and(|character| {
                    character.is_alphanumeric() || character == '_' || character == '.'
                }) {
                    continue;
                }
                let after = rest
                    .trim_start()
                    .strip_prefix('(')
                    .unwrap_or(rest.trim_start())
                    .trim_start();
                let Some(quote) = after
                    .chars()
                    .next()
                    .filter(|character| matches!(character, '"' | '\'' | '`'))
                else {
                    continue;
                };
                if let Some((specifier, _)) = after[1..].split_once(quote) {
                    found.push((specifier.to_owned(), index + 1));
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{is_config_package, specifiers};

    #[test]
    fn specifiers_of_every_import_form() {
        let text = "import a from \"x\";\nimport \"side\";\nconst b = await import('dyn');\nconst c = require( \"req\" );\nexport { d } from 're';\nconst reimport = 1;\n";
        let found: Vec<(&str, usize)> = specifiers(text)
            .iter()
            .map(|(name, line)| (name.as_str(), *line))
            .collect();
        assert_eq!(
            found,
            [("x", 1), ("side", 2), ("dyn", 3), ("req", 4), ("re", 5)]
        );
    }

    #[test]
    fn configuration_packages_by_name() {
        assert!(is_config_package("eslint-config-airbnb"));
        assert!(is_config_package("@scope/eslint-config"));
        assert!(is_config_package("@scope/eslint-config-react/flat"));
        assert!(!is_config_package("@eslint/js"));
        assert!(!is_config_package("eslint-plugin-react"));
    }
}

//! `node/formatter`: one Prettier configuration, a root `.prettierrc.json` with the standard
//! settings.

use gate_sdk::{NoOptions, Result, Rule, Run, Scope, Tree};

use crate::repo::{self, directory, file_name};
use crate::settings::Settings;
use crate::standard::PRETTIER;

#[derive(Debug, Default)]
pub(crate) struct Formatter;

/// The one form of the configuration.
const CONFIG: &str = ".prettierrc.json";

impl Rule for Formatter {
    type Options = NoOptions;
    const NAME: &'static str = "formatter";
    const TITLE: &'static str =
        "One Prettier configuration: a root .prettierrc.json with the standard settings";
    const ASPECTS: &'static [&'static str] = &["form", "nested", "settings"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        Settings::read(run)?;
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        if manifests.is_empty() {
            return Ok(());
        }
        for path in tree.paths() {
            if path == CONFIG || !is_config(file_name(&path)) {
                continue;
            }
            let (aspect, message) = if directory(&path).is_empty() {
                (
                    "form",
                    format!("{path} configures Prettier in another form; the one form is {CONFIG}"),
                )
            } else {
                (
                    "nested",
                    format!(
                        "{path} configures Prettier below the root; a repository has one configuration"
                    ),
                )
            };
            crate::report(
                run.report(),
                aspect,
                (&path, None),
                file_name(&path),
                message,
                "remove it; the root .prettierrc.json applies to every file of the repository",
            );
        }
        for path in &manifests {
            let Some(manifest) = repo::read_json(&tree, path)? else {
                continue;
            };
            if let Some(member) = manifest.member("prettier") {
                let aspect = if path == "package.json" {
                    "form"
                } else {
                    "nested"
                };
                crate::report(
                    run.report(),
                    aspect,
                    (path, Some(member.line)),
                    "prettier",
                    format!(
                        "{path} configures Prettier in its \"prettier\" key; the one form is the root {CONFIG}"
                    ),
                    "move the settings to the root .prettierrc.json and remove the key",
                );
            }
        }
        settings(run, &tree)
    }
}

/// Whether a file of this name configures Prettier.
fn is_config(name: &str) -> bool {
    name == ".prettierrc"
        || name
            .strip_prefix(".prettierrc")
            .or_else(|| name.strip_prefix("prettier.config"))
            .is_some_and(|rest| {
                matches!(
                    rest,
                    ".json"
                        | ".json5"
                        | ".yaml"
                        | ".yml"
                        | ".toml"
                        | ".js"
                        | ".cjs"
                        | ".mjs"
                        | ".ts"
                        | ".cts"
                        | ".mts"
                )
            })
}

/// `settings`: the root configuration holds exactly the standard.
fn settings(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>) -> Result<()> {
    let Some(config) = repo::read_json(tree, CONFIG)? else {
        crate::report(
            run.report(),
            "form",
            (CONFIG, None),
            CONFIG,
            format!("{CONFIG} is not tracked at the repository root"),
            "add .prettierrc.json with the standard settings: {\"printWidth\": 120, \"proseWrap\": \"preserve\", \"trailingComma\": \"all\"}",
        );
        return Ok(());
    };
    for (key, value) in PRETTIER {
        let found = config.member(key);
        if found.is_some_and(|member| member.value.render() == value) {
            continue;
        }
        let message = match found {
            Some(member) => format!(
                "{CONFIG} sets {key} to {}; the standard is {value}",
                member.value.render()
            ),
            None => format!("{CONFIG} does not set {key}; the standard is {value}"),
        };
        crate::report(
            run.report(),
            "settings",
            (CONFIG, found.map(|member| member.line)),
            key,
            message,
            "write the standard value; the formatting of every repository is the same",
        );
    }
    for member in config.members() {
        if member.key == "$schema" || PRETTIER.iter().any(|(key, _)| *key == member.key) {
            continue;
        }
        crate::report(
            run.report(),
            "settings",
            (CONFIG, Some(member.line)),
            &member.key,
            format!(
                "{CONFIG} sets {}, which the standard leaves at Prettier's default",
                member.key
            ),
            "remove the key; the standard sets only printWidth, proseWrap and trailingComma",
        );
    }
    Ok(())
}

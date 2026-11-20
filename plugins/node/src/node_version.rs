//! `node/node-version`: one Node version, in `.nvmrc`, that every `engines.node` admits.

use gate_sdk::{NoOptions, Result, Rule, Run, Scope, Tree};

use crate::repo::{self, file_name};
use crate::semver::{self, Range, Version};
use crate::settings::Settings;

#[derive(Debug, Default)]
pub(crate) struct NodeVersion;

impl Rule for NodeVersion {
    type Options = NoOptions;
    const NAME: &'static str = "node-version";
    const TITLE: &'static str = "One Node version, in .nvmrc, that every engines.node admits";
    const ASPECTS: &'static [&'static str] = &["file", "form", "engines", "differs"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        Settings::read(run)?;
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        if manifests.is_empty() {
            return Ok(());
        }
        let pinned = version_file(run, &tree);
        forms(run, &tree, &manifests)?;
        let Some(root) = repo::read_json(&tree, "package.json")? else {
            crate::report(
                run.report(),
                "engines",
                ("package.json", None),
                "engines.node",
                "the repository root has no package.json to declare engines.node".to_owned(),
                "add a root package.json with \"engines\": { \"node\": \"<range>\" }",
            );
            return Ok(());
        };
        let declared = root
            .get("engines")
            .and_then(|engines| engines.member("node"))
            .and_then(|member| {
                member
                    .value
                    .as_str()
                    .map(|text| (text.to_owned(), member.line))
            });
        let Some((range, line)) = declared else {
            crate::report(
                run.report(),
                "engines",
                ("package.json", None),
                "engines.node",
                "package.json declares no engines.node".to_owned(),
                "write \"engines\": { \"node\": \">=<x.y.z> <<x+1>\" } for the version in .nvmrc",
            );
            return Ok(());
        };
        engines(run, &range, line, pinned);
        for path in manifests
            .iter()
            .filter(|path| path.as_str() != "package.json")
        {
            let Some(manifest) = repo::read_json(&tree, path)? else {
                continue;
            };
            let Some(member) = manifest
                .get("engines")
                .and_then(|engines| engines.member("node"))
            else {
                continue;
            };
            let other = member.value.render();
            if member.value.as_str().map(normal) != Some(normal(&range)) {
                crate::report(
                    run.report(),
                    "differs",
                    (path, Some(member.line)),
                    "engines.node",
                    format!(
                        "{path} declares engines.node {other}, the root package.json \"{range}\""
                    ),
                    "write the root's range, or remove engines.node where the root's applies",
                );
            }
        }
        Ok(())
    }
}

/// `file`: `.nvmrc` at the root holds one exact version; returns it.
fn version_file(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>) -> Option<Version> {
    let text = tree.text(".nvmrc");
    let pinned = text.as_deref().map(str::trim).and_then(semver::exact);
    if pinned.is_none() {
        let message = match text {
            None => ".nvmrc is not tracked at the repository root".to_owned(),
            Some(text) => format!(
                ".nvmrc holds \"{}\", not one exact version x.y.z",
                text.trim()
            ),
        };
        crate::report(
            run.report(),
            "file",
            (".nvmrc", None),
            ".nvmrc",
            message,
            "write the exact Node version, such as 24.11.0, as the only line of .nvmrc",
        );
    }
    pinned
}

/// `form`: no second place names the Node version.
fn forms(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>, manifests: &[String]) -> Result<()> {
    for path in tree.paths() {
        let name = file_name(&path);
        let second = match name {
            ".node-version" => true,
            ".nvmrc" => path != ".nvmrc",
            ".tool-versions" => tree.text(&path).is_some_and(|text| {
                text.lines()
                    .any(|line| matches!(line.split_whitespace().next(), Some("nodejs" | "node")))
            }),
            _ => false,
        };
        if second {
            crate::report(
                run.report(),
                "form",
                (&path, None),
                name,
                format!("{path} names a Node version too; the one place is .nvmrc at the root"),
                "remove it; .nvmrc at the repository root is read by every tool and by CI",
            );
        }
    }
    for path in manifests {
        let Some(manifest) = repo::read_json(tree, path)? else {
            continue;
        };
        if let Some(member) = manifest.get("volta").and_then(|volta| volta.member("node")) {
            crate::report(
                run.report(),
                "form",
                (path, Some(member.line)),
                "volta.node",
                format!("{path} pins Node in volta.node too; the one place is .nvmrc at the root"),
                "remove volta.node",
            );
        }
    }
    Ok(())
}

/// `engines`: the root's range reads as a range and, with an exact `.nvmrc`, admits that version
/// and no earlier major.
fn engines(run: &mut Run<'_, NoOptions>, range: &str, line: usize, pinned: Option<Version>) {
    let problem = match (Range::parse(range), pinned) {
        (None, _) => Some(format!(
            "package.json engines.node \"{range}\" is not a range Gate reads"
        )),
        (Some(parsed), Some(pinned)) if !parsed.admits(pinned) => Some(format!(
            "package.json engines.node \"{range}\" does not admit {}.{}.{}, the version in .nvmrc",
            pinned.major, pinned.minor, pinned.patch
        )),
        (Some(parsed), Some(pinned)) if parsed.admits_below(pinned.major) => Some(format!(
            "package.json engines.node \"{range}\" admits a Node major below {}, the major in .nvmrc",
            pinned.major
        )),
        (Some(_), _) => None,
    };
    if let Some(message) = problem {
        crate::report(
            run.report(),
            "engines",
            ("package.json", Some(line)),
            "engines.node",
            message,
            "write a range from the .nvmrc version to the next major, such as \">=24.11.0 <25\"",
        );
    }
}

/// A range with its white space collapsed, for comparing.
fn normal(range: &str) -> String {
    range.split_whitespace().collect::<Vec<_>>().join(" ")
}

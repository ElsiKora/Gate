//! `node/node-version`: one Node version per install root, in its `.nvmrc`, that every
//! `engines.node` below it admits.

use gate_sdk::{NoOptions, Result, Rule, Run, Scope, Tree};

use crate::repo::{self, file_name, join, root_of};
use crate::semver::{self, Range, Version};
use crate::settings::Settings;

#[derive(Debug, Default)]
pub(crate) struct NodeVersion;

impl Rule for NodeVersion {
    type Options = NoOptions;
    const NAME: &'static str = "node-version";
    const TITLE: &'static str =
        "One Node version per install root, in its .nvmrc, that every engines.node below it admits";
    const ASPECTS: &'static [&'static str] = &["file", "form", "engines", "differs"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let roots = Settings::read(run)?.install_roots();
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        if manifests.is_empty() {
            return Ok(());
        }
        forms(run, &tree, &manifests, &roots)?;
        for root in &roots {
            let mine: Vec<&String> = manifests
                .iter()
                .filter(|path| root_of(path, &roots) == root.as_str())
                .collect();
            if !mine.is_empty() {
                install(run, &tree, root, &mine)?;
            }
        }
        Ok(())
    }
}

/// One install root: its `.nvmrc`, its `engines.node`, and the ranges of the manifests below it.
fn install(
    run: &mut Run<'_, NoOptions>,
    tree: &Tree<'_>,
    root: &str,
    manifests: &[&String],
) -> Result<()> {
    let pinned = version_file(run, tree, root);
    let path = join(root, "package.json");
    let Some(manifest) = repo::read_json(tree, &path)? else {
        crate::report(
            run.report(),
            "engines",
            (&path, None),
            "engines.node",
            format!(
                "{} has no package.json to declare engines.node",
                shown(root)
            ),
            "add a package.json at the install root with \"engines\": { \"node\": \"<range>\" }",
        );
        return Ok(());
    };
    let declared = manifest
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
            (&path, None),
            "engines.node",
            format!("{path} declares no engines.node"),
            "write \"engines\": { \"node\": \">=<x.y.z> <<x+1>\" } for the version in its .nvmrc",
        );
        return Ok(());
    };
    engines(run, &path, &range, line, pinned)?;
    for other in manifests.iter().filter(|other| other.as_str() != path) {
        let Some(manifest) = repo::read_json(tree, other)? else {
            continue;
        };
        let Some(member) = manifest
            .get("engines")
            .and_then(|engines| engines.member("node"))
        else {
            continue;
        };
        let written = member.value.render();
        if member.value.as_str().map(normal) != Some(normal(&range)) {
            crate::report(
                run.report(),
                "differs",
                (other, Some(member.line)),
                "engines.node",
                format!("{other} declares engines.node {written}, {path} \"{range}\""),
                "write the root's range",
            );
        }
    }
    Ok(())
}

/// The install root as a message names it.
fn shown(root: &str) -> String {
    if root.is_empty() {
        "the repository root".to_owned()
    } else {
        format!("the install root {root}")
    }
}

/// `file`: `.nvmrc` at the install root holds one exact version; returns it.
fn version_file(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>, root: &str) -> Option<Version> {
    let path = join(root, ".nvmrc");
    let text = tree.text(&path);
    let pinned = text.as_deref().map(str::trim).and_then(semver::exact);
    if pinned.is_none() {
        let message = match text.as_deref() {
            None => format!("{path} is not tracked: an install root names its Node version there"),
            Some(text) => format!(
                "{path} holds \"{}\", not one exact version x.y.z",
                text.trim()
            ),
        };
        // `v24.11.0` is what `node --version` prints, but the standard's form has no `v`.
        let help = text
            .as_deref()
            .map(str::trim)
            .and_then(|text| text.strip_prefix('v'))
            .filter(|rest| semver::exact(rest).is_some())
            .map_or_else(
                || {
                    "write the exact Node version, such as 24.11.0, as the only line of .nvmrc"
                        .to_owned()
                },
                |rest| format!("drop the v: write {rest} as the only line of .nvmrc"),
            );
        crate::report(
            run.report(),
            "file",
            (&path, None),
            ".nvmrc",
            message,
            &help,
        );
    }
    pinned
}

/// `form`: no second place names the Node version.
fn forms(
    run: &mut Run<'_, NoOptions>,
    tree: &Tree<'_>,
    manifests: &[String],
    roots: &[String],
) -> Result<()> {
    for path in tree.paths() {
        let name = file_name(&path);
        let second = match name {
            ".node-version" => true,
            ".nvmrc" => !roots.iter().any(|root| join(root, ".nvmrc") == path),
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
                format!(
                    "{path} names a Node version too; the one place is .nvmrc at its install root"
                ),
                "remove it; .nvmrc at the install root is read by every tool and by CI",
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
                format!(
                    "{path} pins Node in volta.node too; the one place is .nvmrc at its install root"
                ),
                "remove volta.node",
            );
        }
    }
    Ok(())
}

/// `engines`: the root's range reads as a range and, with an exact `.nvmrc`, admits that version
/// and no earlier major.
fn engines(
    run: &mut Run<'_, NoOptions>,
    path: &str,
    range: &str,
    line: usize,
    pinned: Option<Version>,
) -> Result<()> {
    let problem = match (Range::parse(range)?, pinned) {
        (None, _) => Some(format!(
            "{path} engines.node \"{range}\" is not a range Gate reads"
        )),
        (Some(parsed), Some(pinned)) if !parsed.admits(pinned) => Some(format!(
            "{path} engines.node \"{range}\" does not admit {}.{}.{}, the version in its .nvmrc",
            pinned.major, pinned.minor, pinned.patch
        )),
        (Some(parsed), Some(pinned)) if parsed.admits_below(pinned.major) => Some(format!(
            "{path} engines.node \"{range}\" admits a Node major below {}, the major in its .nvmrc",
            pinned.major
        )),
        (Some(_), _) => None,
    };
    if let Some(message) = problem {
        crate::report(
            run.report(),
            "engines",
            (path, Some(line)),
            "engines.node",
            message,
            "write a range from the .nvmrc version to the next major, such as \">=24.11.0 <25\"",
        );
    }
    Ok(())
}

/// A range with its white space collapsed, for comparing.
fn normal(range: &str) -> String {
    range.split_whitespace().collect::<Vec<_>>().join(" ")
}

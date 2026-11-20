//! `node/package-manager`: one package manager, pnpm, pinned at each install root.

use gate_sdk::{Error, NoOptions, Result, Rule, Run, Scope, Tree};

use crate::repo::{self, directory, file_name, join, root_of};
use crate::settings::Settings;
use crate::standard::{OTHER_LOCKFILES, PACKAGE_MANAGER};
use crate::yaml;

#[derive(Debug, Default)]
pub(crate) struct PackageManager;

impl Rule for PackageManager {
    type Options = NoOptions;
    const NAME: &'static str = "package-manager";
    const TITLE: &'static str = "One package manager, pnpm, pinned at each install root";
    const ASPECTS: &'static [&'static str] =
        &["field", "lockfile", "nested", "members", "save-exact"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        if manifests.is_empty() {
            return Ok(());
        }
        let roots = settings.install_roots();
        for root in &roots {
            field(run, &tree, root)?;
            save_exact(run, &tree, root)?;
            if !tree.contains(&join(root, "pnpm-lock.yaml")) {
                let path = join(root, "pnpm-lock.yaml");
                crate::report(
                    run.report(),
                    "lockfile",
                    (&path, None),
                    "pnpm-lock.yaml",
                    format!("{path} is not tracked: an install root keeps its lockfile in git"),
                    "run pnpm install at the install root and commit pnpm-lock.yaml",
                );
            }
        }
        for path in tree.paths() {
            let name = file_name(&path);
            if OTHER_LOCKFILES.contains(&name) {
                crate::report(
                    run.report(),
                    "lockfile",
                    (&path, None),
                    name,
                    format!(
                        "{path} is the lockfile of another package manager; the one package manager is pnpm"
                    ),
                    "remove it and install with pnpm, which writes pnpm-lock.yaml at the install root",
                );
            } else if matches!(name, "pnpm-lock.yaml" | "pnpm-workspace.yaml")
                && !roots.iter().any(|root| root.as_str() == directory(&path))
            {
                crate::report(
                    run.report(),
                    "nested",
                    (&path, None),
                    name,
                    format!(
                        "{path} is below an install root: a workspace has one {name}, at its root"
                    ),
                    "list the directory in the root pnpm-workspace.yaml and remove the file, or declare \
                     the directory in [plugins.node] options separate with its job",
                );
            }
        }
        members(run, &tree, &roots, &manifests)
    }
}

/// `field`: the root manifest pins pnpm by an exact version.
fn field(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>, root: &str) -> Result<()> {
    let path = join(root, "package.json");
    let Some(manifest) = repo::read_json(tree, &path)? else {
        crate::report(
            run.report(),
            "field",
            (&path, None),
            "packageManager",
            format!(
                "{path} is not tracked: an install root has a package.json that pins its package manager"
            ),
            "add a package.json with \"private\": true and \"packageManager\": \"pnpm@<x.y.z>\"",
        );
        return Ok(());
    };
    let pinned = repo::string_member(&manifest, "packageManager");
    let exact = pinned.is_some_and(|(value, _)| {
        value
            .strip_prefix(PACKAGE_MANAGER)
            .and_then(|rest| rest.strip_prefix('@'))
            .map(|version| {
                version
                    .split_once('+')
                    .map_or(version, |(version, _)| version)
            })
            .and_then(crate::semver::exact)
            .is_some()
    });
    if !exact {
        let (message, line) = match pinned {
            Some((value, line)) => (
                format!(
                    "{path} pins \"packageManager\": \"{value}\", not pnpm at one exact version"
                ),
                Some(line),
            ),
            None => (format!("{path} has no \"packageManager\""), None),
        };
        crate::report(
            run.report(),
            "field",
            (&path, line),
            "packageManager",
            message,
            "write \"packageManager\": \"pnpm@<x.y.z>\" with the exact version every install uses",
        );
    }
    Ok(())
}

/// `save-exact`: installs write exact versions. pnpm reads `save-exact` and `save-prefix` from the
/// install root's `.npmrc`, where the last assignment of a key wins, and `saveExact` and
/// `savePrefix` from its `pnpm-workspace.yaml`, which win over `.npmrc`. An install writes exact
/// versions when `save-exact` is `true` or `save-prefix` is empty.
fn save_exact(run: &mut Run<'_, NoOptions>, tree: &Tree<'_>, root: &str) -> Result<()> {
    let path = join(root, ".npmrc");
    let mut exact = None;
    let mut prefix = None;
    for line in tree.text(&path).unwrap_or_default().lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = unquote(value.trim());
        match key.trim() {
            "save-exact" => exact = Some(value == "true"),
            "save-prefix" => prefix = Some(value.to_owned()),
            _ => {}
        }
    }
    let workspace = join(root, "pnpm-workspace.yaml");
    if let Some(text) = tree.text(&workspace) {
        let document = yaml::parse(&text).map_err(|message| {
            Error::new(format!("{workspace} is not YAML ({message}); fix it"))
        })?;
        if let Some(value) = document.get("saveExact").and_then(yaml::Node::scalar) {
            exact = Some(value == "true");
        }
        if let Some(value) = document.get("savePrefix").and_then(yaml::Node::scalar) {
            prefix = Some(value.to_owned());
        }
    }
    if exact != Some(true) && prefix.as_deref() != Some("") {
        crate::report(
            run.report(),
            "save-exact",
            (&path, None),
            "save-exact",
            format!(
                "{path} does not set save-exact=true (nor {workspace} saveExact: true, nor an \
                 empty save prefix), so an install may write a range"
            ),
            "add the line save-exact=true to .npmrc at the install root, after any other \
             save-exact line",
        );
    }
    Ok(())
}

/// `value` without one pair of surrounding quotes, as npm reads an `.npmrc` value.
fn unquote(value: &str) -> &str {
    ['"', '\'']
        .iter()
        .find_map(|quote| {
            value
                .strip_prefix(*quote)
                .and_then(|rest| rest.strip_suffix(*quote))
        })
        .unwrap_or(value)
}

/// `members`: every manifest below an install root is a member of that root's workspace.
fn members(
    run: &mut Run<'_, NoOptions>,
    tree: &Tree<'_>,
    roots: &[String],
    manifests: &[String],
) -> Result<()> {
    for root in roots {
        let workspace = repo::workspace(tree, root)?;
        for path in manifests {
            let place = directory(path);
            if place == root.as_str() || root_of(path, roots) != root.as_str() {
                continue;
            }
            let relative = if root.is_empty() {
                place
            } else {
                &place[root.len() + 1..]
            };
            if workspace
                .as_ref()
                .is_some_and(|workspace| workspace.has_member(relative))
            {
                continue;
            }
            let workspace_path = join(root, "pnpm-workspace.yaml");
            crate::report(
                run.report(),
                "members",
                (path, None),
                "package.json",
                format!(
                    "{path} is in no package glob of {workspace_path}, so no install covers it"
                ),
                "add its directory, or a glob that matches it, to packages in pnpm-workspace.yaml",
            );
        }
    }
    Ok(())
}

//! `node/dependency-versions`: one version per dependency, written exactly; in a workspace, every
//! external dependency comes from the catalog and every member through `workspace:*`.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::{NoOptions, Result, Rule, Run, Scope, Tree};

use crate::json::Node;
use crate::repo::{self, CatalogEntry, Workspace, root_of};
use crate::semver::is_exact;
use crate::settings::Settings;

#[derive(Debug, Default)]
pub(crate) struct DependencyVersions;

/// The sections that install a version; `peerDependencies` may hold ranges.
const SECTIONS: [&str; 3] = ["dependencies", "devDependencies", "optionalDependencies"];

/// One dependency as a manifest writes it.
struct Entry {
    path: String,
    section: &'static str,
    name: String,
    spec: String,
    line: usize,
}

impl Rule for DependencyVersions {
    type Options = NoOptions;
    const NAME: &'static str = "dependency-versions";
    const TITLE: &'static str = "One version per dependency, written exactly";
    const ASPECTS: &'static [&'static str] = &["range", "catalog", "internal", "differs"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let manifests = repo::manifests(&tree);
        let roots = settings.install_roots();
        for root in &roots {
            let mine: Vec<&String> = manifests
                .iter()
                .filter(|path| root_of(path, &roots) == root.as_str())
                .collect();
            if mine.is_empty() {
                continue;
            }
            let (entries, members) = read(&tree, &mine)?;
            let workspace = repo::workspace(&tree, root)?;
            match workspace {
                Some(workspace) => {
                    for entry in &workspace.catalog {
                        let CatalogEntry {
                            name,
                            version,
                            line,
                            ..
                        } = entry;
                        if !is_exact(version) {
                            crate::report(
                                run.report(),
                                "range",
                                (&workspace.path, Some(*line)),
                                name,
                                format!(
                                    "the catalog of {} pins {name} at \"{version}\", not one exact version",
                                    workspace.path
                                ),
                                "write the exact version x.y.z the workspace installs",
                            );
                        }
                    }
                    for entry in &entries {
                        in_workspace(run, entry, &members, &workspace);
                    }
                }
                None => alone(run, &entries),
            }
        }
        Ok(())
    }
}

/// Every installed dependency of `manifests`, and the names of the packages they declare.
fn read(tree: &Tree<'_>, manifests: &[&String]) -> Result<(Vec<Entry>, BTreeSet<String>)> {
    let mut entries = Vec::new();
    let mut members = BTreeSet::new();
    for path in manifests {
        let Some(manifest) = repo::read_json(tree, path)? else {
            continue;
        };
        if let Some((name, _)) = repo::string_member(&manifest, "name") {
            members.insert(name.to_owned());
        }
        for section in SECTIONS {
            for member in manifest.get(section).map(Node::members).unwrap_or_default() {
                entries.push(Entry {
                    path: (*path).clone(),
                    section,
                    name: member.key.clone(),
                    spec: member.value.as_str().unwrap_or_default().to_owned(),
                    line: member.line,
                });
            }
        }
    }
    Ok((entries, members))
}

/// `catalog` and `internal`: a member's dependencies in a workspace.
fn in_workspace(
    run: &mut Run<'_, NoOptions>,
    entry: &Entry,
    members: &BTreeSet<String>,
    workspace: &Workspace,
) {
    let catalog = workspace.has_catalog;
    let Entry {
        path,
        section,
        name,
        spec,
        line,
    } = entry;
    if members.contains(name) || spec.starts_with("workspace:") {
        if spec != "workspace:*" {
            let message = if members.contains(name) {
                format!(
                    "{path} {section} takes the workspace package {name} as \"{spec}\", not \"workspace:*\""
                )
            } else {
                format!(
                    "{path} {section} takes {name} as \"{spec}\", but no package of the workspace is named {name}"
                )
            };
            crate::report(
                run.report(),
                "internal",
                (path, Some(*line)),
                name,
                message,
                "write \"workspace:*\" for a package of the workspace, and a catalog entry for any other",
            );
        }
        return;
    }
    if let Some(which) = spec.strip_prefix("catalog:")
        && catalog
    {
        let which = if which.is_empty() { "default" } else { which };
        if !workspace.has_entry(which, name) {
            crate::report(
                run.report(),
                "catalog",
                (path, Some(*line)),
                name,
                format!(
                    "{path} {section} takes {name} from the {which} catalog of {}, which has no entry for it",
                    workspace.path
                ),
                "add the package with its exact version to that catalog in pnpm-workspace.yaml",
            );
        }
        return;
    }
    let message = if catalog {
        format!("{path} {section} pins {name} at \"{spec}\" instead of taking it from the catalog")
    } else {
        format!("{path} {section} pins {name} at \"{spec}\", and the workspace has no catalog")
    };
    crate::report(
        run.report(),
        "catalog",
        (path, Some(*line)),
        name,
        message,
        "add the exact version to catalog in pnpm-workspace.yaml and write \"catalog:\" here",
    );
}

/// `range` and `differs`: the dependencies of an install root without a workspace.
fn alone(run: &mut Run<'_, NoOptions>, entries: &[Entry]) {
    let mut first: BTreeMap<&str, &Entry> = BTreeMap::new();
    for entry in entries {
        if !is_exact(&entry.spec) {
            crate::report(
                run.report(),
                "range",
                (&entry.path, Some(entry.line)),
                &entry.name,
                format!(
                    "{} {} takes {} as \"{}\", not one exact version",
                    entry.path, entry.section, entry.name, entry.spec
                ),
                "write the exact version x.y.z that is installed; save-exact=true keeps it so",
            );
            continue;
        }
        match first.get(entry.name.as_str()) {
            Some(earlier) if earlier.spec != entry.spec => crate::report(
                run.report(),
                "differs",
                (&entry.path, Some(entry.line)),
                &entry.name,
                format!(
                    "{} takes {} at {}, {} at {}",
                    entry.path, entry.name, entry.spec, earlier.path, earlier.spec
                ),
                "use one version of a dependency in the whole repository",
            ),
            Some(_) => {}
            None => {
                first.insert(&entry.name, entry);
            }
        }
    }
}

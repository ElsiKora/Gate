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
                        if let Some(help) = Spec::of(version).fix() {
                            crate::report(
                                run.report(),
                                "range",
                                (&workspace.path, Some(*line)),
                                name,
                                format!(
                                    "the catalog of {} pins {name} at \"{version}\", not one exact version",
                                    workspace.path
                                ),
                                &help,
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
    let kind = Spec::of(spec);
    if matches!(kind, Spec::Local) {
        return;
    }
    if let (Spec::Path(_), Some(help)) = (&kind, kind.fix()) {
        crate::report(
            run.report(),
            "range",
            (path, Some(*line)),
            name,
            format!("{path} {section} takes {name} as \"{spec}\", not one exact version"),
            &help,
        );
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
        let spec = Spec::of(&entry.spec);
        if let Some(help) = spec.fix() {
            crate::report(
                run.report(),
                "range",
                (&entry.path, Some(entry.line)),
                &entry.name,
                format!(
                    "{} {} takes {} as \"{}\", not one exact version",
                    entry.path, entry.section, entry.name, entry.spec
                ),
                &help,
            );
            continue;
        }
        if matches!(spec, Spec::Local) {
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

/// What a dependency's specifier names, for `range`.
#[derive(Debug, PartialEq, Eq)]
enum Spec<'a> {
    /// One version: `x.y.z`, `npm:<name>@x.y.z`, or a git URL pinned to a full commit.
    Exact,
    /// A package of the tree: `workspace:`, `file:` or `link:`. Not a range.
    Local,
    /// `npm:<name>@<range>`: an alias whose version is not exact.
    Alias(&'a str),
    /// A git URL that is not pinned to a full commit; the URL without its `#` part.
    Git(&'a str),
    /// `catalog:` without a pnpm workspace to hold the catalog.
    Catalog,
    /// A bare path (`./x`, `../x`): a local package, written without its `file:` protocol.
    Path(&'a str),
    /// A range, a tag or anything else.
    Range,
}

impl<'a> Spec<'a> {
    fn of(spec: &'a str) -> Self {
        if is_exact(spec) {
            return Self::Exact;
        }
        if ["workspace:", "file:", "link:"]
            .iter()
            .any(|prefix| spec.starts_with(prefix))
        {
            return Self::Local;
        }
        if spec.starts_with("catalog:") {
            return Self::Catalog;
        }
        if spec.starts_with("./") || spec.starts_with("../") {
            return Self::Path(spec);
        }
        if let Some(alias) = spec.strip_prefix("npm:") {
            return if split_alias(alias).is_some_and(|(_, version)| is_exact(version)) {
                Self::Exact
            } else {
                Self::Alias(alias)
            };
        }
        if is_git(spec) {
            let (url, fragment) = spec.split_once('#').unwrap_or((spec, ""));
            let commit = fragment.strip_prefix("commit=").unwrap_or(fragment);
            return if commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()) {
                Self::Exact
            } else {
                Self::Git(url)
            };
        }
        Self::Range
    }

    /// The help for a specifier that is not one exact version; `None` when it is one, or local.
    fn fix(&self) -> Option<String> {
        match self {
            Self::Exact | Self::Local => None,
            Self::Alias(alias) => {
                let name = split_alias(alias).map_or(*alias, |(name, _)| name);
                Some(format!(
                    "write npm:{name}@x.y.z with the exact version that is installed"
                ))
            }
            Self::Git(url) => Some(format!(
                "pin it to a full 40-character commit: {url}#<commit>"
            )),
            Self::Path(path) => Some(format!(
                "write file:{path}; a local package is named with the file: protocol"
            )),
            Self::Catalog => Some(
                "catalog: needs a pnpm workspace that declares the catalog; write the exact \
                 version x.y.z here"
                    .to_owned(),
            ),
            Self::Range => Some(
                "write the exact version x.y.z that is installed; save-exact=true keeps it so"
                    .to_owned(),
            ),
        }
    }
}

/// An alias `<name>@<version>` split at its version's `@`; a scoped name starts with `@`, so the
/// search starts after the first character.
fn split_alias(alias: &str) -> Option<(&str, &str)> {
    let at = alias.get(1..)?.rfind('@')? + 1;
    Some((&alias[..at], &alias[at + 1..]))
}

/// Whether `spec` names a git repository: a `git` URL, a URL ending in `.git`, a host shorthand
/// (`github:`, `gitlab:`, `bitbucket:`), or `<owner>/<repo>` as npm reads it.
fn is_git(spec: &str) -> bool {
    let url = spec.split_once('#').map_or(spec, |(url, _)| url);
    spec.starts_with("git+")
        || spec.starts_with("git://")
        || ["github:", "gitlab:", "bitbucket:"]
            .iter()
            .any(|prefix| spec.starts_with(prefix))
        || (url.starts_with("https://")
            && std::path::Path::new(url)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("git")))
        || (!url.contains(':')
            && !url.starts_with('@')
            && url.split('/').count() == 2
            && url.split('/').all(|part| !part.is_empty()))
}

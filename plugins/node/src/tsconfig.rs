//! `node/tsconfig`: every `tsconfig*.json` of the project reaches its install root's base through
//! `extends`; none weakens it; module resolution is the kind's. The base's own flags are
//! `node/tsconfig-base`'s.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::{NoOptions, Result, Rule, Run};

use crate::json::{Node, Value};
use crate::repo::{self, directory, file_name, join, normalize, within};
use crate::settings::{Kind, Settings};
use crate::standard::{Row, STRICT_FAMILY, STRICT_FLAGS, difference, resolution};

type Options = NoOptions;

#[derive(Debug, Default)]
pub(crate) struct Tsconfig;

/// How many standard flags a configuration without `extends` may set before it is a copy of the
/// base.
const COPY_THRESHOLD: usize = 3;

impl Rule for Tsconfig {
    type Options = Options;
    const NAME: &'static str = "tsconfig";
    const TITLE: &'static str =
        "Every tsconfig reaches its install root's base and none weakens it";
    const ASPECTS: &'static [&'static str] = &["base", "weakened", "resolution", "copy"];

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let settings = Settings::read(run)?;
        let kind = settings.kind;
        let tree = run.tree();
        let project = run.project().to_owned();
        let base_path = settings.base_of(&project);
        let mut configs = BTreeMap::new();
        for path in repo::own_paths(&tree) {
            let name = file_name(&path);
            if name.starts_with("tsconfig")
                && repo::is_json(name)
                && let Some(config) = repo::read_json(&tree, &path)?
            {
                configs.insert(path, config);
            }
        }
        if configs.is_empty() {
            return Ok(());
        }
        let own_base = within(&base_path, &project).then(|| relative(&base_path, &project));
        for (path, config) in &configs {
            // The base is `node/tsconfig-base`'s, checked once per install from the repository.
            if own_base.as_deref() == Some(path.as_str()) {
                continue;
            }
            weakened(run, path, config);
            resolutions(run, path, config, kind);
            // A solution-style configuration (`files: []`, no `include`) compiles nothing: its
            // references are the configurations that compile, and they are checked as their own.
            if compiles_nothing(config) {
                continue;
            }
            if config.get("extends").is_none() && strict_count(config) >= COPY_THRESHOLD {
                crate::report(
                    run.report(),
                    "copy",
                    (path, None),
                    file_name(path),
                    format!("{path} repeats the base's flags instead of extending {base_path}"),
                    "extend the base and keep only what this configuration adds",
                );
            }
            let line = config.member("extends").map(|member| member.line);
            match reaches(&configs, path, &project, &base_path) {
                Reach::Base => {}
                Reach::Nowhere => crate::report(
                    run.report(),
                    "base",
                    (path, line),
                    file_name(path),
                    format!("{path} does not reach {base_path} through extends"),
                    "extend the base, directly or through a configuration that does",
                ),
                Reach::Leaves(outside) => crate::report(
                    run.report(),
                    "base",
                    (path, line),
                    file_name(path),
                    format!(
                        "{path} extends through {outside}, outside the project, so its chain to \
                         {base_path} cannot be followed"
                    ),
                    &format!(
                        "extend the base directly: \"extends\": \"{}\"",
                        up_to(directory(&join(&project, path)), &base_path)
                    ),
                ),
            }
        }
        if let Some(difference) = difference(kind, Row::Resolution) {
            for start in resolution_roots(&configs) {
                if sets_resolution(&configs, &start, own_base.as_deref()) {
                    continue;
                }
                crate::report(
                    run.report(),
                    "resolution",
                    (&start, None),
                    "moduleResolution",
                    format!(
                        "{start} inherits the base's moduleResolution; a {} has {}: {}",
                        kind.name(),
                        difference.cell,
                        difference.reason
                    ),
                    &format!("set module and moduleResolution in {start}"),
                );
            }
        }
        Ok(())
    }
}

/// `path` relative to `project`.
fn relative(path: &str, project: &str) -> String {
    if project.is_empty() {
        return path.to_owned();
    }
    path.strip_prefix(project)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path)
        .to_owned()
}

/// The members of `config`'s `compilerOptions`.
pub(crate) fn options(config: &Node) -> &[crate::json::Member] {
    config
        .get("compilerOptions")
        .map(Node::members)
        .unwrap_or_default()
}

fn strict_count(config: &Node) -> usize {
    options(config)
        .iter()
        .filter(|member| {
            STRICT_FLAGS.contains(&member.key.as_str()) && member.value.as_bool() == Some(true)
        })
        .count()
}

/// `weakened`: a standard flag, or one `strict` turns on, set to `false`.
fn weakened(run: &mut Run<'_, Options>, path: &str, config: &Node) {
    for member in options(config) {
        let flag = member.key.as_str();
        if member.value.as_bool() == Some(false)
            && (STRICT_FLAGS.contains(&flag) || STRICT_FAMILY.contains(&flag))
        {
            crate::report(
                run.report(),
                "weakened",
                (path, Some(member.line)),
                flag,
                format!("{path} turns {flag} off, which the standard keeps on"),
                "remove the line; an exception with its reason is the way to relax a flag for a file",
            );
        }
    }
}

/// `resolution`: `module` and `moduleResolution`, where set, are the kind's.
fn resolutions(run: &mut Run<'_, Options>, path: &str, config: &Node, kind: Kind) {
    let (module, resolve) = resolution(kind);
    for (flag, expected) in [("module", module), ("moduleResolution", resolve)] {
        let Some(member) = options(config).iter().find(|member| member.key == flag) else {
            continue;
        };
        let value = member.value.as_str().unwrap_or_default();
        if !value.eq_ignore_ascii_case(expected) {
            crate::report(
                run.report(),
                "resolution",
                (path, Some(member.line)),
                flag,
                format!(
                    "{path} sets {flag} to {}; a {} uses {expected}",
                    member.value.render(),
                    kind.name()
                ),
                "write the kind's value, or leave it to the base where the kind's is the base's",
            );
        }
    }
}

/// The `extends` of `config`: a string or a list of strings.
fn extends(config: &Node) -> Vec<&str> {
    match config.get("extends").map(|node| &node.value) {
        Some(Value::String(text)) => vec![text.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(Node::as_str).collect(),
        _ => Vec::new(),
    }
}

/// The repository path an `extends` of the configuration at `path` names, when it is relative.
///
/// As TypeScript resolves it: the path as written, then with `.json` added, then, for a directory,
/// its `tsconfig.json`; the first that `exists` is the target. A package specifier names an
/// installed package, which is not in the tree, so it has no target here.
fn target(
    project: &str,
    path: &str,
    specifier: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }
    let written = normalize(&join(project, &join(directory(path), specifier)))?;
    let candidates = [
        written.clone(),
        format!("{written}.json"),
        join(&written, "tsconfig.json"),
    ];
    candidates
        .into_iter()
        .find(|candidate| exists(candidate.as_str()))
}

/// Where a configuration's `extends` chain goes.
#[derive(Debug, PartialEq, Eq)]
enum Reach {
    /// It reaches the base.
    Base,
    /// It leaves the project through a configuration the project cannot read: the path named.
    Leaves(String),
    /// It ends without reaching the base.
    Nowhere,
}

/// Where the configuration at `start` goes through `extends`.
fn reaches(configs: &BTreeMap<String, Node>, start: &str, project: &str, base: &str) -> Reach {
    let mut seen = BTreeSet::new();
    let mut queue = vec![start.to_owned()];
    let mut leaves = None;
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(config) = configs.get(&path) else {
            continue;
        };
        for specifier in extends(config) {
            let exists = |candidate: &str| {
                candidate == base
                    || (within(candidate, project)
                        && configs.contains_key(&relative(candidate, project)))
            };
            let Some(repository) = target(project, &path, specifier, &exists) else {
                let written = specifier
                    .starts_with('.')
                    .then(|| normalize(&join(project, &join(directory(&path), specifier))))
                    .flatten();
                if let Some(written) = written.filter(|written| !within(written, project)) {
                    leaves.get_or_insert(written);
                }
                continue;
            };
            if repository == base {
                return Reach::Base;
            }
            if within(&repository, project) {
                queue.push(relative(&repository, project));
            }
        }
    }
    leaves.map_or(Reach::Nowhere, Reach::Leaves)
}

/// The relative path from the repository directory `from` to the repository path `to`.
fn up_to(from: &str, to: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|part| !part.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec![".."; from.len() - common];
    parts.extend(&to_parts[common..]);
    let path = parts.join("/");
    if path.starts_with("..") {
        path
    } else {
        format!("./{path}")
    }
}

/// Whether `config` is solution-style: `files` is empty and there is no `include`, so it compiles
/// nothing itself and only refers to the configurations that do.
fn compiles_nothing(config: &Node) -> bool {
    matches!(config.get("files").map(|node| &node.value), Some(Value::Array(items)) if items.is_empty())
        && config.get("include").is_none()
}

/// The configurations whose resolution a kind with its own resolution must set: the project's
/// `tsconfig.json`, or, when it compiles nothing, each configuration it refers to.
fn resolution_roots(configs: &BTreeMap<String, Node>) -> Vec<String> {
    let Some(root) = configs.get("tsconfig.json") else {
        return Vec::new();
    };
    if !compiles_nothing(root) {
        return vec!["tsconfig.json".to_owned()];
    }
    let references = match root.get("references").map(|node| &node.value) {
        Some(Value::Array(items)) => items.as_slice(),
        _ => &[],
    };
    references
        .iter()
        .filter_map(|reference| reference.get("path").and_then(Node::as_str))
        .filter_map(|written| {
            let written = if written.starts_with('.') {
                written.to_owned()
            } else {
                format!("./{written}")
            };
            target("", "tsconfig.json", &written, &|candidate: &str| {
                configs.contains_key(candidate)
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Whether the configuration at `start`, or one it extends inside the project, sets
/// `moduleResolution`. The base the project owns does not count: it holds the common resolution,
/// which the kind's difference replaces.
fn sets_resolution(configs: &BTreeMap<String, Node>, start: &str, base: Option<&str>) -> bool {
    let mut seen = BTreeSet::new();
    let mut queue = vec![start.to_owned()];
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) || base == Some(path.as_str()) {
            continue;
        }
        let Some(config) = configs.get(&path) else {
            continue;
        };
        if options(config)
            .iter()
            .any(|member| member.key == "moduleResolution")
        {
            return true;
        }
        for specifier in extends(config) {
            if let Some(next) = target("", &path, specifier, &|candidate: &str| {
                configs.contains_key(candidate)
            }) {
                queue.push(next);
            }
        }
    }
    false
}

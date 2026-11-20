//! `node/tsconfig`: one base per repository; every `tsconfig*.json` reaches it through `extends`;
//! none weakens it; module resolution is the kind's.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Result, Rule, Run};

use crate::json::{Node, Value};
use crate::repo::{self, directory, file_name, join, normalize, within};
use crate::settings::{Kind, Settings};
use crate::standard::{Row, STRICT_FAMILY, STRICT_FLAGS, difference, resolution};

#[derive(Debug, Default)]
pub(crate) struct Tsconfig;

/// The options of `node/tsconfig`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub(crate) struct Options {
    /// The base's path from the repository root.
    #[serde(default = "base")]
    base: String,
}

fn base() -> String {
    "tsconfig.base.json".to_owned()
}

/// How many standard flags a configuration without `extends` may set before it is a copy of the
/// base.
const COPY_THRESHOLD: usize = 3;

impl Rule for Tsconfig {
    type Options = Options;
    const NAME: &'static str = "tsconfig";
    const TITLE: &'static str = "Every tsconfig reaches the repository's base and none weakens it";
    const ASPECTS: &'static [&'static str] = &["base", "weakened", "flags", "resolution", "copy"];

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let kind = Settings::read(run)?.kind;
        let tree = run.tree();
        let project = run.project().to_owned();
        let base_path = run.options().base.clone();
        let mut configs = BTreeMap::new();
        for path in tree.paths() {
            let name = file_name(&path);
            if name.starts_with("tsconfig")
                && name.ends_with(".json")
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
            weakened(run, path, config);
            if own_base.as_deref() == Some(path.as_str()) {
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
            if !reaches(&configs, path, &project, &base_path) {
                crate::report(
                    run.report(),
                    "base",
                    (path, config.member("extends").map(|member| member.line)),
                    file_name(path),
                    format!("{path} does not reach {base_path} through extends"),
                    "extend the base, directly or through a configuration that does",
                );
            }
            resolutions(run, path, config, kind);
        }
        if let Some(own) = &own_base {
            flags(run, own, configs.get(own));
        }
        if let Some(difference) = difference(kind, Row::Resolution)
            && configs.contains_key("tsconfig.json")
            && !sets_resolution(&configs, "tsconfig.json")
        {
            crate::report(
                run.report(),
                "resolution",
                ("tsconfig.json", None),
                "moduleResolution",
                format!(
                    "tsconfig.json inherits the base's moduleResolution; a {} has {}: {}",
                    kind.name(),
                    difference.cell,
                    difference.reason
                ),
                "set module and moduleResolution in the project's tsconfig.json",
            );
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

fn options(config: &Node) -> &[crate::json::Member] {
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

/// `flags`: the base sets every standard flag and the common resolution.
fn flags(run: &mut Run<'_, Options>, path: &str, base: Option<&Node>) {
    let Some(base) = base else {
        crate::report(
            run.report(),
            "flags",
            (path, None),
            file_name(path),
            format!("{path}, the repository's tsconfig base, is not tracked"),
            "add the base with the standard flags; every tsconfig extends it",
        );
        return;
    };
    let set = options(base);
    let value = |flag: &str| set.iter().find(|member| member.key == flag);
    for flag in STRICT_FLAGS {
        if value(flag).and_then(|member| member.value.as_bool()) != Some(true) {
            crate::report(
                run.report(),
                "flags",
                (path, value(flag).map(|member| member.line)),
                flag,
                format!("the base {path} does not set {flag} to true"),
                "set it to true; the base carries every standard flag",
            );
        }
    }
    for flag in ["module", "moduleResolution"] {
        let found = value(flag);
        if !found
            .and_then(|member| member.value.as_str())
            .is_some_and(|text| text.eq_ignore_ascii_case("NodeNext"))
        {
            crate::report(
                run.report(),
                "flags",
                (path, found.map(|member| member.line)),
                flag,
                format!("the base {path} does not set {flag} to NodeNext"),
                "set it to NodeNext; a frontend overrides it with Bundler in its own tsconfig.json",
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
fn target(project: &str, path: &str, specifier: &str) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }
    let mut resolved = normalize(&join(project, &join(directory(path), specifier)))?;
    if !resolved.ends_with(".json") {
        resolved.push_str(".json");
    }
    Some(resolved)
}

/// Whether the configuration at `start` reaches the base through `extends`.
fn reaches(configs: &BTreeMap<String, Node>, start: &str, project: &str, base: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut queue = vec![start.to_owned()];
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(config) = configs.get(&path) else {
            continue;
        };
        for specifier in extends(config) {
            let Some(repository) = target(project, &path, specifier) else {
                continue;
            };
            if repository == base {
                return true;
            }
            if within(&repository, project) {
                queue.push(relative(&repository, project));
            }
        }
    }
    false
}

/// Whether the configuration at `start`, or one it extends inside the project, sets
/// `moduleResolution`.
fn sets_resolution(configs: &BTreeMap<String, Node>, start: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut queue = vec![start.to_owned()];
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
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
            if let Some(next) = target("", &path, specifier) {
                queue.push(next);
            }
        }
    }
    false
}

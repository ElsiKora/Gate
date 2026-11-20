//! Paths, manifests and workspaces of the repository, as the rules read them.

use gate_sdk::{Error, Result, Tree};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::json::{self, Node};
use crate::yaml;

/// The directory of `path`; `""` for a file at the top.
pub(crate) fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(directory, _)| directory)
}

/// The last segment of `path`.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `name` inside `directory`.
pub(crate) fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_owned()
    } else {
        format!("{directory}/{name}")
    }
}

/// `path` with its `.` and `..` segments resolved; `None` when it climbs above the top.
pub(crate) fn normalize(path: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    Some(segments.join("/"))
}

/// Whether `path` is `directory` itself or inside it; every path is inside `""`.
pub(crate) fn within(path: &str, directory: &str) -> bool {
    directory.is_empty()
        || path == directory
        || path
            .strip_prefix(directory)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The deepest of `roots` that holds `path`.
pub(crate) fn root_of<'r>(path: &str, roots: &'r [String]) -> &'r str {
    roots
        .iter()
        .filter(|root| within(path, root))
        .max_by_key(|root| root.len())
        .map_or("", String::as_str)
}

/// Every tracked `package.json`, outside `node_modules`.
pub(crate) fn manifests(tree: &Tree<'_>) -> Vec<String> {
    tree.paths()
        .into_iter()
        .filter(|path| file_name(path) == "package.json")
        .filter(|path| !path.split('/').any(|segment| segment == "node_modules"))
        .collect()
}

/// The project's own paths: those whose nearest `package.json` above them is the project's, not a
/// nested package's. A project at the repository root is granted the whole tree, and its members'
/// files are theirs to check, against their own kind.
pub(crate) fn own_paths(tree: &Tree<'_>) -> Vec<String> {
    let packages: Vec<String> = manifests(tree)
        .iter()
        .map(|path| directory(path).to_owned())
        .filter(|place| !place.is_empty())
        .collect();
    tree.paths()
        .into_iter()
        .filter(|path| !packages.iter().any(|place| within(path, place)))
        .collect()
}

/// The JSON of tracked `path`, when it is tracked.
///
/// # Errors
/// When the file is not UTF-8 or not JSON: the call fails, naming the file and the line.
pub(crate) fn read_json(tree: &Tree<'_>, path: &str) -> Result<Option<Node>> {
    let Some(bytes) = tree.bytes(path) else {
        return Ok(None);
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| Error::new(format!("{path} is not UTF-8 text; save it as UTF-8")))?;
    json::parse(&text).map(Some).map_err(|message| {
        Error::new(format!(
            "{path} is not JSON ({message}); fix it so the node rules can read it"
        ))
    })
}

/// A `pnpm-workspace.yaml`: its package globs and its catalog.
#[derive(Debug)]
pub(crate) struct Workspace {
    /// The file's path.
    pub(crate) path: String,
    included: GlobSet,
    excluded: GlobSet,
    /// Whether it has a `catalog` or `catalogs`.
    pub(crate) has_catalog: bool,
    /// Every catalog entry: its name, version and line.
    pub(crate) catalog: Vec<(String, String, usize)>,
}

impl Workspace {
    /// Whether the package in `directory` (relative to the workspace's root) is a member.
    pub(crate) fn has_member(&self, directory: &str) -> bool {
        self.included.is_match(directory) && !self.excluded.is_match(directory)
    }
}

/// The `pnpm-workspace.yaml` of install root `root`, when it is tracked.
///
/// # Errors
/// When the file is not YAML, or a package glob is not a glob: the call fails, naming the file.
pub(crate) fn workspace(tree: &Tree<'_>, root: &str) -> Result<Option<Workspace>> {
    let path = join(root, "pnpm-workspace.yaml");
    let Some(text) = tree.text(&path) else {
        return Ok(None);
    };
    let document = yaml::parse(&text)
        .map_err(|message| Error::new(format!("{path} is not YAML ({message}); fix it")))?;
    let mut included = GlobSetBuilder::new();
    let mut excluded = GlobSetBuilder::new();
    for item in document
        .get("packages")
        .and_then(yaml::Node::sequence)
        .unwrap_or_default()
    {
        let Some(pattern) = item.scalar() else {
            continue;
        };
        let (set, pattern) = match pattern.strip_prefix('!') {
            Some(negated) => (&mut excluded, negated),
            None => (&mut included, pattern),
        };
        let pattern = pattern.trim_start_matches("./").trim_end_matches('/');
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| {
                Error::new(format!(
                    "{path} line {}: \"{pattern}\" is not a glob ({error}); write one such as \"packages/*\"",
                    item.line
                ))
            })?;
        set.add(glob);
    }
    let build = |builder: GlobSetBuilder| {
        builder.build().map_err(|error| {
            Error::new(format!("{path}: the package globs do not build ({error})"))
        })
    };
    let mut catalog = Vec::new();
    let mut has_catalog = false;
    if let Some(entries) = document.get("catalog").and_then(yaml::Node::mapping) {
        has_catalog = true;
        collect(entries, &mut catalog);
    }
    if let Some(named) = document.get("catalogs").and_then(yaml::Node::mapping) {
        has_catalog = true;
        for (_, entries) in named {
            if let Some(entries) = entries.mapping() {
                collect(entries, &mut catalog);
            }
        }
    }
    Ok(Some(Workspace {
        included: build(included)?,
        excluded: build(excluded)?,
        path,
        has_catalog,
        catalog,
    }))
}

fn collect(entries: &[(yaml::Node, yaml::Node)], catalog: &mut Vec<(String, String, usize)>) {
    for entry in entries {
        if let (Some(name), Some(version)) = (entry.0.scalar(), entry.1.scalar()) {
            catalog.push((name.to_owned(), version.to_owned(), entry.1.line));
        }
    }
}

/// The string value of member `key` of `node`, with the member's line.
pub(crate) fn string_member<'n>(node: &'n Node, key: &str) -> Option<(&'n str, usize)> {
    let member = node.member(key)?;
    member.value.as_str().map(|text| (text, member.line))
}

#[cfg(test)]
mod tests {
    use super::{normalize, root_of, within};

    #[test]
    fn paths_resolve_and_roots_nest() {
        assert_eq!(normalize("a/./b/../c").as_deref(), Some("a/c"));
        assert_eq!(normalize("a/../../b"), None);
        assert!(within("apps/a/x", "apps/a"));
        assert!(!within("apps/ab/x", "apps/a"));
        let roots = vec![String::new(), "spikes/x".to_owned()];
        assert_eq!(root_of("spikes/x/package.json", &roots), "spikes/x");
        assert_eq!(root_of("spikes/xy/package.json", &roots), "");
    }
}

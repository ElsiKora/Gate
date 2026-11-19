//! Projects: the directories of a repository that have rules of their own.
//!
//! A project comes from the package manager's workspace (`pnpm`, `npm`), from explicit globs, or
//! from a tracked `gate.toml` below the root. The root itself is a project when the root
//! `gate.toml` has no `[workspace]`, or when the workspace lists `"."`. A path belongs to the
//! project with the longest root that contains it; a path of no project belongs to the repository
//! root. Cargo and Go workspaces
//! are further [`Origin`]s with their own discovery function next to [`pnpm_globs`] and
//! [`npm_globs`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Serialize;

use crate::config::{self, Config, ProjectName, ProjectSources, WorkspaceSection};
use crate::error::{Error, Result};
use crate::snapshot::{Snapshot, directory_prefix};

/// Why a directory is a project.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// The repository top, because the root `gate.toml` has no `[workspace]`.
    Root,
    /// A tracked `gate.toml` in the directory.
    GateToml,
    /// The `packages` of `pnpm-workspace.yaml`.
    Pnpm,
    /// The `workspaces` of the root `package.json`.
    Npm,
    /// A glob of `[workspace] projects`.
    Glob,
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Root => "no workspace",
            Self::GateToml => "gate.toml",
            Self::Pnpm => "pnpm",
            Self::Npm => "npm",
            Self::Glob => "glob",
        })
    }
}

/// One project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Project {
    /// The name: `[project] name`, else the `package.json` name, else the root (`"."` for the
    /// top).
    pub name: String,
    /// The root directory, relative to the repository root; `""` for the top.
    pub root: String,
    /// Why it is a project.
    pub origins: BTreeSet<Origin>,
    /// The path of its `gate.toml`, when it has one.
    pub config: Option<String>,
    /// The `name` of its `package.json`, when it has one.
    pub package: Option<String>,
    /// The names of the workspace projects it depends on.
    pub dependencies: BTreeSet<String>,
}

impl Project {
    /// Whether `path` (repository-relative) is inside the project's root.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        path.starts_with(&directory_prefix(&self.root))
    }

    /// `path` relative to the project root, when it is inside.
    #[must_use]
    pub fn relative<'a>(&self, path: &'a str) -> Option<&'a str> {
        path.strip_prefix(&directory_prefix(&self.root))
    }
}

/// The projects of a repository, sorted by name.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Projects {
    list: Vec<Project>,
}

impl Projects {
    /// Every project, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = &Project> {
        self.list.iter()
    }

    /// The number of projects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether there is no project.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The project named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Project> {
        self.list.iter().find(|project| project.name == name)
    }

    /// The project that owns `path`: the one with the longest root containing it. `None` when the
    /// path belongs to the repository root.
    #[must_use]
    pub fn owner(&self, path: &str) -> Option<&Project> {
        self.list
            .iter()
            .filter(|project| project.contains(path))
            .max_by_key(|project| project.root.len())
    }

    /// Every project that depends on `name`, directly or through other projects, sorted by name,
    /// each with the dependency through which it was reached.
    #[must_use]
    pub fn dependents(&self, name: &str) -> BTreeMap<String, String> {
        let mut reached = BTreeMap::new();
        let mut queue = VecDeque::from([name.to_owned()]);

        while let Some(current) = queue.pop_front() {
            for project in &self.list {
                if project.dependencies.contains(&current)
                    && project.name != name
                    && !reached.contains_key(&project.name)
                {
                    reached.insert(project.name.clone(), current.clone());
                    queue.push_back(project.name.clone());
                }
            }
        }

        reached
    }

    /// Builds the list from discovered roots. Only for tests and tools that already know the
    /// projects; a run uses [`discover`].
    #[must_use]
    pub fn from_list(mut list: Vec<Project>) -> Self {
        list.sort_by(|left, right| left.name.cmp(&right.name));
        Self { list }
    }
}

/// The tracked `gate.toml` files below the root, outside ignored directories.
///
/// # Errors
/// Fails when an ignore glob does not compile.
pub fn project_config_paths(
    snapshot: &Snapshot,
    workspace: Option<&WorkspaceSection>,
) -> Result<Vec<String>> {
    tracked_named(snapshot, workspace, config::FILE_NAME)
}

/// The tracked files named `name` below the root, outside ignored directories.
///
/// # Errors
/// Fails when an ignore glob does not compile.
pub fn tracked_named(
    snapshot: &Snapshot,
    workspace: Option<&WorkspaceSection>,
    name: &str,
) -> Result<Vec<String>> {
    let ignore = ignore_set(workspace)?;
    let suffix = format!("/{name}");
    Ok(snapshot
        .paths()
        .filter_map(|path| {
            let directory = path.strip_suffix(&suffix)?;
            (!is_ignored(&ignore, directory)).then(|| path.to_owned())
        })
        .collect())
}

/// Every project root (`""` for the top) with the sources that make it one, before `ignore`.
fn project_roots(
    snapshot: &Snapshot,
    workspace: Option<&WorkspaceSection>,
    configs: &BTreeMap<String, Config>,
) -> Result<BTreeMap<String, BTreeSet<Origin>>> {
    let mut roots: BTreeMap<String, BTreeSet<Origin>> = BTreeMap::new();

    if workspace.is_none() {
        roots.entry(String::new()).or_default().insert(Origin::Root);
    }

    for directory in configs.keys() {
        roots
            .entry(directory.clone())
            .or_default()
            .insert(Origin::GateToml);
    }

    if let Some(sources) = workspace.and_then(|workspace| workspace.projects.as_ref()) {
        let (origin, globs) = match sources {
            ProjectSources::Pnpm => (Origin::Pnpm, pnpm_globs(snapshot)?),
            ProjectSources::Npm => (Origin::Npm, npm_globs(snapshot)?),
            ProjectSources::Globs(globs) => (Origin::Glob, globs.clone()),
        };
        let matcher = ProjectGlobs::compile(&globs).map_err(|message| {
            Error::config(format!("gate.toml: [workspace] projects: {message}"))
        })?;
        if matcher.matches("") {
            roots.entry(String::new()).or_default().insert(origin);
        }
        let candidates: BTreeSet<String> = match origin {
            // A package manager's project is a directory with a package.json.
            Origin::Pnpm | Origin::Npm => snapshot
                .paths()
                .filter_map(|path| path.strip_suffix("/package.json"))
                .map(str::to_owned)
                .collect(),
            Origin::Glob | Origin::GateToml | Origin::Root => directories(snapshot),
        };

        for directory in candidates {
            if matcher.matches(&directory) {
                roots.entry(directory).or_default().insert(origin);
            }
        }
    }

    Ok(roots)
}

/// Discovers the projects of a snapshot.
///
/// `root` is the root `gate.toml`; `configs` are the loaded project `gate.toml` files, keyed by
/// their directory.
///
/// # Errors
/// Fails when the workspace file the configuration names is missing or malformed, or two projects
/// share a name.
pub fn discover(
    snapshot: &Snapshot,
    root: &Config,
    configs: &BTreeMap<String, Config>,
) -> Result<Projects> {
    let workspace = root.file().workspace.as_ref();
    let ignore = ignore_set(workspace)?;
    let roots = project_roots(snapshot, workspace, configs)?;
    let mut list = Vec::new();

    for (directory, origins) in roots {
        if !directory.is_empty() && is_ignored(&ignore, &directory) {
            continue;
        }

        let config = if directory.is_empty() {
            Some(root)
        } else {
            configs.get(&directory)
        };
        let manifest = read_package(snapshot, &directory)?;
        let name = config
            .and_then(|config| config.file().project.as_ref())
            .and_then(|project| project.name.as_ref())
            .map(|name| name.as_str().to_owned())
            .or_else(|| {
                manifest
                    .as_ref()
                    .and_then(|manifest| manifest.name.clone())
                    .filter(|name| ProjectName::is_valid(name))
            })
            .unwrap_or_else(|| {
                if directory.is_empty() {
                    ".".to_owned()
                } else {
                    directory.clone()
                }
            });

        list.push((
            Project {
                name,
                root: directory,
                origins,
                config: config.map(|config| config.path().to_owned()),
                package: manifest.as_ref().and_then(|manifest| manifest.name.clone()),
                dependencies: BTreeSet::new(),
            },
            manifest
                .map(|manifest| manifest.dependencies)
                .unwrap_or_default(),
        ));
    }

    let mut names: BTreeMap<&str, &str> = BTreeMap::new();
    for (project, _) in &list {
        if let Some(other) = names.insert(&project.name, &project.root) {
            return Err(Error::config(format!(
                "two projects are named {}: {} and {}; give one of them [project] name = \"...\" in its gate.toml",
                project.name,
                display_root(other),
                display_root(&project.root)
            )));
        }
    }

    let by_package: BTreeMap<String, String> = list
        .iter()
        .filter_map(|(project, _)| {
            project
                .package
                .clone()
                .map(|package| (package, project.name.clone()))
        })
        .collect();

    let projects = list
        .into_iter()
        .map(|(mut project, dependencies)| {
            project.dependencies = dependencies
                .iter()
                .filter_map(|package| by_package.get(package))
                .filter(|name| **name != project.name)
                .cloned()
                .collect();
            project
        })
        .collect();

    Ok(Projects::from_list(projects))
}

/// The `packages` globs of `pnpm-workspace.yaml`.
///
/// # Errors
/// Fails when the file is not tracked or is not a mapping with a list of strings under `packages`.
pub fn pnpm_globs(snapshot: &Snapshot) -> Result<Vec<String>> {
    const FILE: &str = "pnpm-workspace.yaml";
    let text = snapshot.text(FILE)?.ok_or_else(|| {
        Error::config(format!(
            "gate.toml: [workspace] projects = \"pnpm\" but {FILE} is not tracked; add it, or list globs"
        ))
    })?;
    let documents = yaml_rust2::YamlLoader::load_from_str(&text)
        .map_err(|error| Error::config(format!("{FILE}: {error}")))?;
    let packages = documents
        .first()
        .map(|document| &document["packages"])
        .and_then(yaml_rust2::Yaml::as_vec)
        .ok_or_else(|| {
            Error::config(format!(
                "{FILE}: packages is missing or not a list; list the globs of the workspace packages"
            ))
        })?;

    packages
        .iter()
        .map(|item| {
            item.as_str().map(str::to_owned).ok_or_else(|| {
                Error::config(format!("{FILE}: every entry of packages must be a string"))
            })
        })
        .collect()
}

/// The `workspaces` globs of the root `package.json` (a list, or `{ "packages": [...] }`).
///
/// # Errors
/// Fails when the file is not tracked, not JSON, or has no `workspaces` list.
pub fn npm_globs(snapshot: &Snapshot) -> Result<Vec<String>> {
    const FILE: &str = "package.json";
    let text = snapshot.text(FILE)?.ok_or_else(|| {
        Error::config(format!(
            "gate.toml: [workspace] projects = \"npm\" but the root {FILE} is not tracked; add it, or list globs"
        ))
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| Error::config(format!("{FILE}: {error}")))?;
    let workspaces = &value["workspaces"];
    let list = workspaces
        .as_array()
        .or_else(|| workspaces["packages"].as_array())
        .ok_or_else(|| {
            Error::config(format!(
                "{FILE}: workspaces is missing; list the workspace globs, or set [workspace] projects to globs"
            ))
        })?;

    list.iter()
        .map(|item| {
            item.as_str().map(str::to_owned).ok_or_else(|| {
                Error::config(format!(
                    "{FILE}: every entry of workspaces must be a string"
                ))
            })
        })
        .collect()
}

struct Manifest {
    name: Option<String>,
    dependencies: BTreeSet<String>,
}

fn read_package(snapshot: &Snapshot, root: &str) -> Result<Option<Manifest>> {
    let path = config::join(root, "package.json");
    let Some(text) = snapshot.text(&path)? else {
        return Ok(None);
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| Error::config(format!("{path}: {error}")))?;
    let mut dependencies = BTreeSet::new();

    for field in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(map) = value[field].as_object() {
            dependencies.extend(map.keys().cloned());
        }
    }

    Ok(Some(Manifest {
        name: value["name"].as_str().map(str::to_owned),
        dependencies,
    }))
}

/// Every directory that holds a tracked path, the root excluded.
fn directories(snapshot: &Snapshot) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();

    for path in snapshot.paths() {
        let mut current = path;
        while let Some((parent, _)) = current.rsplit_once('/') {
            if !directories.insert(parent.to_owned()) {
                break;
            }
            current = parent;
        }
    }

    directories
}

fn normalize(glob: &str) -> &str {
    glob.strip_prefix("./")
        .unwrap_or(glob)
        .trim_end_matches('/')
}

/// A root directory as reports print it: `"."` for the top.
#[must_use]
pub fn display_root(root: &str) -> &str {
    if root.is_empty() { "." } else { root }
}

/// Globs of project roots, `!` excluding, as `[workspace] projects` and `[[profiles]] projects`
/// write them. `"."` is the repository top.
#[derive(Clone, Debug)]
pub struct ProjectGlobs {
    include: GlobSet,
    exclude: GlobSet,
    top: bool,
    top_excluded: bool,
}

impl ProjectGlobs {
    /// Compiles the globs.
    ///
    /// # Errors
    /// Fails, naming the glob, when one does not compile.
    pub fn compile(globs: &[String]) -> std::result::Result<Self, String> {
        let mut include = GlobSetBuilder::new();
        let mut exclude = GlobSetBuilder::new();
        let mut top = false;
        let mut top_excluded = false;

        for glob in globs {
            let (negated, pattern) = match glob.strip_prefix('!') {
                Some(pattern) => (true, pattern),
                None => (false, glob.as_str()),
            };
            let pattern = normalize(pattern);

            if pattern == "." || pattern.is_empty() {
                if negated {
                    top_excluded = true;
                } else {
                    top = true;
                }
                continue;
            }

            let compiled = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| format!("{glob:?} is not a glob: {error}"))?;

            if negated {
                exclude.add(compiled);
            } else {
                include.add(compiled);
            }
        }

        let build = |builder: GlobSetBuilder| builder.build().map_err(|error| error.to_string());
        Ok(Self {
            include: build(include)?,
            exclude: build(exclude)?,
            top,
            top_excluded,
        })
    }

    /// Whether the directory (repository-relative, `""` for the top) matches.
    #[must_use]
    pub fn matches(&self, directory: &str) -> bool {
        if directory.is_empty() {
            self.top && !self.top_excluded
        } else {
            self.include.is_match(directory) && !self.exclude.is_match(directory)
        }
    }
}

fn ignore_set(workspace: Option<&WorkspaceSection>) -> Result<GlobSet> {
    let globs = workspace
        .map(|workspace| workspace.ignore.as_slice())
        .unwrap_or_default();
    let mut builder = GlobSetBuilder::new();

    for glob in globs {
        let pattern = normalize(glob);
        // "<dir>/**" ignores <dir> itself too, not only what is below it.
        let patterns = std::iter::once(pattern).chain(pattern.strip_suffix("/**"));
        for pattern in patterns {
            let compiled = GlobBuilder::new(pattern)
                .literal_separator(true)
                .build()
                .map_err(|error| {
                    Error::config(format!(
                        "gate.toml: [workspace] ignore: {glob:?} is not a glob: {error}"
                    ))
                })?;
            builder.add(compiled);
        }
    }

    builder
        .build()
        .map_err(|error| Error::config(format!("gate.toml: [workspace] ignore: {error}")))
}

/// Whether `directory` or one of its parents matches an ignore glob.
fn is_ignored(ignore: &GlobSet, directory: &str) -> bool {
    let mut current = directory;
    loop {
        if ignore.is_match(current) {
            return true;
        }
        match current.rsplit_once('/') {
            Some((parent, _)) => current = parent,
            None => return false,
        }
    }
}

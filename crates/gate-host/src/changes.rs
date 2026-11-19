//! Change selection: which paths a change touches, and which projects therefore run.

use std::collections::BTreeSet;
use std::fmt;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::git::{Git, ZERO_OID};
use crate::projects::Projects;
use crate::snapshot::Source;
use crate::workspace::Inputs;

/// The paths a change touches.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangeSet {
    /// How the change was taken, for the report: `staged`, `changed since <base>`, `push <range>`.
    pub description: String,
    /// The changed paths (added, modified, deleted; renames as both paths), sorted.
    pub paths: BTreeSet<String>,
    /// Every path counts as changed: a pushed branch with no common history with its remote.
    pub everything: bool,
}

/// A `--push <base>..<tip>` range, resolved: the plugin interface's `commit-range`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushRange {
    /// The base commit; `None` for a new branch (`0000...`).
    pub base: Option<String>,
    /// The tip commit; the run's snapshot is its tree.
    pub tip: String,
    /// The remote ref the range is pushed to (`--target-ref`), a full ref such as `refs/heads/main`.
    pub target_ref: Option<String>,
}

impl ChangeSet {
    /// The paths the index changes against `HEAD` (every staged path on an unborn branch).
    ///
    /// # Errors
    /// Fails when git fails.
    pub fn staged(git: &Git) -> Result<Self> {
        let mut arguments = vec!["diff", "--cached", "--name-only", "-z", "--no-renames"];
        let head = git.head()?;
        if let Some(head) = &head {
            arguments.push(head);
        }
        Ok(Self {
            description: "staged".to_owned(),
            paths: git.paths(&arguments)?.into_iter().collect(),
            everything: false,
        })
    }

    /// The paths that differ between the common ancestor of `base` and the snapshot's commit, and
    /// the snapshot itself (the working tree, the index, or the commit).
    ///
    /// # Errors
    /// Fails when `base` names no commit, the snapshot has no commit, or git fails.
    pub fn since(git: &Git, base: &str, source: &Source) -> Result<Self> {
        let base_commit = git.resolve_commit(base)?.ok_or_else(|| {
            Error::environment(format!(
                "--base {base} names no commit; fetch it (git fetch) or pass another revision"
            ))
        })?;
        let tip = match source {
            Source::Commit(revision) => git.resolve_commit(revision)?,
            Source::WorkTree | Source::Index => git.head()?,
        }
        .ok_or_else(|| {
            Error::environment("--changed needs a commit to compare with; make the first commit")
        })?;
        let description = format!("changed since {base}");

        let Some(ancestor) = git.merge_base(&base_commit, &tip)? else {
            return Ok(Self {
                description,
                paths: BTreeSet::new(),
                everything: true,
            });
        };

        let mut arguments = vec!["diff", "--name-only", "-z", "--no-renames"];
        match source {
            Source::WorkTree => arguments.push(&ancestor),
            Source::Index => {
                arguments.push("--cached");
                arguments.push(&ancestor);
            }
            Source::Commit(_) => {
                arguments.push(&ancestor);
                arguments.push(&tip);
            }
        }

        Ok(Self {
            description,
            paths: git.paths(&arguments)?.into_iter().collect(),
            everything: false,
        })
    }

    /// The paths a push of `range` changes: `<base>..<tip>`, or `<base>` alone for `<base>..HEAD`.
    /// A base of all zeros is a new branch: every path counts as changed. `target_ref`, the remote
    /// ref pushed to, is carried into the range.
    ///
    /// # Errors
    /// Fails when the range is symmetric (`...`), a side names no commit, or git fails.
    pub fn push(git: &Git, range: &str, target_ref: Option<&str>) -> Result<(Self, PushRange)> {
        if range.contains("...") {
            return Err(Error::config(format!(
                "--push {range}: a symmetric range (...) is not a push; write <base>..<tip>"
            )));
        }
        let (base, tip) = range.split_once("..").unwrap_or((range, "HEAD"));
        let tip = if tip.is_empty() { "HEAD" } else { tip };
        let description = format!("push {range}");
        let tip_commit = git
            .resolve_commit(tip)?
            .ok_or_else(|| Error::environment(format!("--push {range}: {tip} names no commit")))?;

        if base.is_empty() || base == ZERO_OID {
            return Ok((
                Self {
                    description,
                    paths: BTreeSet::new(),
                    everything: true,
                },
                PushRange {
                    base: None,
                    tip: tip_commit,
                    target_ref: target_ref.map(str::to_owned),
                },
            ));
        }

        let base_commit = git.resolve_commit(base)?.ok_or_else(|| {
            Error::environment(format!(
                "--push {range}: {base} names no commit; fetch the remote branch first"
            ))
        })?;
        let range_value = PushRange {
            base: Some(base_commit.clone()),
            tip: tip_commit.clone(),
            target_ref: target_ref.map(str::to_owned),
        };

        let Some(ancestor) = git.merge_base(&base_commit, &tip_commit)? else {
            return Ok((
                Self {
                    description,
                    paths: BTreeSet::new(),
                    everything: true,
                },
                range_value,
            ));
        };

        let paths = git
            .paths(&[
                "diff",
                "--name-only",
                "-z",
                "--no-renames",
                &ancestor,
                &tip_commit,
            ])?
            .into_iter()
            .collect();
        Ok((
            Self {
                description,
                paths,
                everything: false,
            },
            range_value,
        ))
    }

    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.everything && self.paths.is_empty()
    }
}

/// Why a project runs or is skipped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Reason {
    /// The run has no change selection: every project runs.
    Full,
    /// Paths of the project changed: the first one and how many in all.
    Changed {
        /// The first changed path, in sorted order.
        path: String,
        /// How many of the project's paths changed.
        count: usize,
    },
    /// The project depends on a selected project.
    DependsOn {
        /// The selected project it depends on (directly or through others).
        project: String,
    },
    /// A file the repository's configuration was read from changed (the root `gate.toml`, an options
    /// file it names, a workspace manifest, the baseline), which can change every project.
    Configuration {
        /// The first such path.
        path: String,
    },
    /// A file the project's configuration was read from, or a path its plugins read, changed.
    Input {
        /// The first such path.
        path: String,
    },
    /// Every path counts as changed (a new branch, unrelated history).
    Everything,
    /// Nothing the project owns changed, and it depends on nothing that did.
    NoChange,
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => formatter.write_str("full run"),
            Self::Changed { path, count } if *count > 1 => {
                write!(formatter, "changed {path} and {} more", count - 1)
            }
            Self::Changed { path, .. } => write!(formatter, "changed {path}"),
            Self::DependsOn { project } => write!(formatter, "depends on {project}"),
            Self::Configuration { path } => write!(formatter, "configuration input {path} changed"),
            Self::Input { path } => write!(formatter, "input {path} changed"),
            Self::Everything => formatter.write_str("every path counts as changed"),
            Self::NoChange => formatter.write_str("no change"),
        }
    }
}

/// A project and why it runs or is skipped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Decision {
    /// The project's name.
    pub project: String,
    /// Why.
    pub reason: Reason,
}

/// The projects a run covers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Selection {
    /// The projects that run, sorted by name.
    pub ran: Vec<Decision>,
    /// The projects that are skipped, sorted by name.
    pub skipped: Vec<Decision>,
    /// Whether repository-scope rules run: always in a full run, else when any path changed.
    pub repository: bool,
}

impl Selection {
    /// Every project runs.
    #[must_use]
    pub fn full(projects: &Projects) -> Self {
        Self {
            ran: projects
                .iter()
                .map(|project| Decision {
                    project: project.name.clone(),
                    reason: Reason::Full,
                })
                .collect(),
            skipped: Vec::new(),
            repository: true,
        }
    }

    /// The projects `changes` touches (paths they own, or their configuration `inputs`), and those
    /// that depend on them; every project when an input of the repository's configuration changed.
    #[must_use]
    pub fn select(projects: &Projects, changes: &ChangeSet, inputs: &Inputs) -> Self {
        let everywhere = inputs.everywhere_changed(&changes.paths);
        if changes.everything || everywhere.is_some() {
            let reason = match everywhere {
                Some(path) if !changes.everything => Reason::Configuration {
                    path: path.to_owned(),
                },
                _ => Reason::Everything,
            };
            return Self {
                ran: projects
                    .iter()
                    .map(|project| Decision {
                        project: project.name.clone(),
                        reason: reason.clone(),
                    })
                    .collect(),
                skipped: Vec::new(),
                repository: true,
            };
        }

        let mut direct: std::collections::BTreeMap<String, (String, usize)> =
            std::collections::BTreeMap::new();

        for path in &changes.paths {
            if let Some(project) = projects.owner(path) {
                direct
                    .entry(project.name.clone())
                    .and_modify(|(_, count)| *count += 1)
                    .or_insert_with(|| (path.clone(), 1));
            }
        }

        let mut reasons: std::collections::BTreeMap<String, Reason> = direct
            .iter()
            .map(|(name, (path, count))| {
                (
                    name.clone(),
                    Reason::Changed {
                        path: path.clone(),
                        count: *count,
                    },
                )
            })
            .collect();

        for project in projects.iter() {
            if !reasons.contains_key(&project.name)
                && let Some(path) = inputs.project_changed(&project.name, &changes.paths)
            {
                reasons.insert(
                    project.name.clone(),
                    Reason::Input {
                        path: path.to_owned(),
                    },
                );
            }
        }

        let selected: Vec<String> = reasons.keys().cloned().collect();
        for name in &selected {
            for (dependent, through) in projects.dependents(name) {
                reasons
                    .entry(dependent)
                    .or_insert(Reason::DependsOn { project: through });
            }
        }

        let mut ran = Vec::new();
        let mut skipped = Vec::new();

        for project in projects.iter() {
            match reasons.remove(&project.name) {
                Some(reason) => ran.push(Decision {
                    project: project.name.clone(),
                    reason,
                }),
                None => skipped.push(Decision {
                    project: project.name.clone(),
                    reason: Reason::NoChange,
                }),
            }
        }

        Self {
            ran,
            skipped,
            repository: !changes.is_empty(),
        }
    }

    /// Whether the project named `name` runs.
    #[must_use]
    pub fn runs(&self, name: &str) -> bool {
        self.ran.iter().any(|decision| decision.project == name)
    }
}

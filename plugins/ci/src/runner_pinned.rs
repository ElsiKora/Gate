//! `ci/runner-pinned`: no job runs on a moving runner label such as `ubuntu-latest`.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Rule, Run, Scope};
use globset::{Glob, GlobSetBuilder};

use crate::workflows::{default_workflows, is_expression, load};
use gate_sdk::yaml::Node;

/// The options of `ci/runner-pinned`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// A glob, relative to the repository, of the workflow files to read. `*` does not cross a
    /// `/`; `**` does; `{a,b}` is either.
    pub workflows: String,
    /// Globs of the runner labels that move to a new image without a commit.
    pub moving: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            workflows: default_workflows(),
            moving: vec!["*-latest".to_owned()],
        }
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct RunnerPinned;

impl Rule for RunnerPinned {
    type Options = Options;

    const NAME: &'static str = "runner-pinned";
    const TITLE: &'static str = "Jobs run on a versioned runner image, never a moving label";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &run.options().moving {
            let glob = Glob::new(pattern).map_err(|error| {
                Error::config(format!(
                    "moving: {pattern:?} is not a valid glob ({error}); write one such as \
                     \"*-latest\""
                ))
            })?;
            builder.add(glob);
        }
        let moving = builder.build().map_err(|error| {
            Error::config(format!("moving is not a valid set of globs ({error})"))
        })?;
        let workflows = load(&run.tree(), &run.options().workflows)?;

        for workflow in &workflows {
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                let Some(runs_on) = job.get("runs-on") else {
                    continue;
                };
                for label in labels(runs_on) {
                    let Some(text) = label.scalar() else {
                        continue;
                    };
                    if is_expression(text) || !moving.is_match(text) {
                        continue;
                    }
                    run.report()
                        .finding(
                            "",
                            Location::line(&workflow.path, label.line),
                            format!(
                                "job `{id}` runs on `{text}`, a label that moves to new images"
                            ),
                        )
                        .key(format!("{id}:{text}"))
                        .target(format!("{}:{id}", workflow.path))
                        .help(
                            "name the image version (`ubuntu-24.04`, `macos-15`), so a new runner \
                             image arrives in a commit, as an action update does",
                        );
                }
            }
        }

        Ok(())
    }
}

/// The labels of a `runs-on`: one label, a list, or a mapping's `labels`.
fn labels(runs_on: &Node) -> Vec<&Node> {
    if runs_on.scalar().is_some() {
        return vec![runs_on];
    }
    if let Some(items) = runs_on.sequence() {
        return items.iter().collect();
    }
    runs_on.get("labels").map_or_else(Vec::new, labels)
}

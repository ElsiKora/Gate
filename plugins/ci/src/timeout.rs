//! `ci/timeout`: every job that runs steps sets `timeout-minutes`, at most `max-minutes` when set.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{default_workflows, load};

/// The options of `ci/timeout`.
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
    /// The longest `timeout-minutes` a job may set; unset, any.
    pub max_minutes: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            workflows: default_workflows(),
            max_minutes: None,
        }
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct Timeout;

impl Rule for Timeout {
    type Options = Options;

    const NAME: &'static str = "timeout";
    const TITLE: &'static str = "Every job that runs steps sets timeout-minutes";
    const ASPECTS: &'static [&'static str] = &["missing", "over-max"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;
        let max = run.options().max_minutes;

        for workflow in &workflows {
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                if job.get("steps").is_none() {
                    continue;
                }
                let Some(timeout) = job.get("timeout-minutes") else {
                    run.report()
                        .finding(
                            "missing",
                            Location::line(&workflow.path, job.line),
                            format!("job `{id}` sets no timeout-minutes"),
                        )
                        .key(id)
                        .target(format!("{}:{id}", workflow.path))
                        .help(
                            "set `timeout-minutes` a little above the job's usual time; without \
                             it a hung step holds a runner for six hours",
                        );
                    continue;
                };
                let Some(max) = max else {
                    continue;
                };
                let Some(minutes) = timeout.scalar().and_then(|text| text.parse::<u64>().ok())
                else {
                    continue;
                };
                if minutes > max {
                    run.report()
                        .finding(
                            "over-max",
                            Location::line(&workflow.path, timeout.line),
                            format!("job `{id}` sets timeout-minutes {minutes}, above {max}"),
                        )
                        .key(id)
                        .target(format!("{}:{id}", workflow.path))
                        .help("lower timeout-minutes to max-minutes or less, or split the job");
                }
            }
        }

        Ok(())
    }
}

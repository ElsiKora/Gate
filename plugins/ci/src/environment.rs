//! `ci/environment`: a job that can write outside the checks runs in an environment.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{default_workflows, load};

/// The options of `ci/environment`.
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
    /// The token scopes whose `write` puts a job behind an environment.
    pub scopes: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            workflows: default_workflows(),
            scopes: ["contents", "deployments", "id-token", "packages"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct Environment;

impl Rule for Environment {
    type Options = Options;

    const NAME: &'static str = "environment";
    const TITLE: &'static str = "A job that can write outside the checks (a release, a deploy, a publish) runs in an environment";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;
        let guarded = run.options().scopes.clone();

        for workflow in &workflows {
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                // A job that calls a reusable workflow cannot set an environment; the called
                // workflow's jobs can, and are read where they are.
                if job.get("environment").is_some() || job.get("uses").is_some() {
                    continue;
                }
                let Some(permissions) = job.get("permissions") else {
                    continue;
                };
                let written: Vec<&str> = if permissions.scalar() == Some("write-all") {
                    guarded.iter().map(String::as_str).collect()
                } else {
                    permissions
                        .mapping()
                        .unwrap_or_default()
                        .iter()
                        .filter(|(_, access)| access.scalar() == Some("write"))
                        .filter_map(|(granted, _)| granted.scalar())
                        .filter(|granted| guarded.iter().any(|listed| listed == granted))
                        .collect()
                };
                if written.is_empty() {
                    continue;
                }
                run.report()
                    .finding(
                        "",
                        Location::line(&workflow.path, permissions.line),
                        format!(
                            "job `{id}` writes `{}` and runs in no environment",
                            written.join("`, `")
                        ),
                    )
                    .key(id)
                    .target(format!("{}:{id}", workflow.path))
                    .help(
                        "add `environment:` with an environment whose protection rules (required \
                         reviewers, the branches or tags that may deploy) guard what the job \
                         writes",
                    );
            }
        }

        Ok(())
    }
}

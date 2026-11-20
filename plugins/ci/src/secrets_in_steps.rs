//! `ci/secrets-in-steps`: a secret reaches only the step that uses it.

use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{Options, Workflow, is_expression, load};
use gate_sdk::yaml::Node;

/// The rule.
#[derive(Debug, Default)]
pub struct SecretsInSteps;

impl Rule for SecretsInSteps {
    type Options = Options;

    const NAME: &'static str = "secrets-in-steps";
    const TITLE: &'static str =
        "Secrets are passed to the steps that use them, never to a whole workflow or job";
    const ASPECTS: &'static [&'static str] = &["workflow-env", "job-env", "inherit"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;

        for workflow in &workflows {
            if let Some(env) = workflow.root.get("env") {
                for (name, line) in secret_variables(env) {
                    run.report()
                        .finding(
                            "workflow-env",
                            Location::line(&workflow.path, line),
                            format!("the workflow's env `{name}` holds a secret every job sees"),
                        )
                        .key(name)
                        .target(workflow.path.clone())
                        .help(HELP);
                }
            }
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                if let Some(env) = job.get("env") {
                    for (name, line) in secret_variables(env) {
                        run.report()
                            .finding(
                                "job-env",
                                Location::line(&workflow.path, line),
                                format!("job `{id}`'s env `{name}` holds a secret every step sees"),
                            )
                            .key(format!("{id}:{name}"))
                            .target(format!("{}:{id}", workflow.path))
                            .help(HELP);
                    }
                }
                inherit(run, workflow, job, id);
            }
        }

        Ok(())
    }
}

const HELP: &str = "move the variable to the `env` (or `with`) of the step that needs it, so no \
                    other step, and no action it runs, can read the secret";

/// The variables of `env` whose value names a secret, with the line of the value.
fn secret_variables(env: &Node) -> Vec<(String, usize)> {
    env.mapping()
        .unwrap_or_default()
        .iter()
        .filter(|(_, value)| {
            value
                .scalar()
                .is_some_and(|text| is_expression(text) && text.contains("secrets."))
        })
        .map(|(name, value)| (name.scalar().unwrap_or("").to_owned(), value.line))
        .collect()
}

/// Reports a reusable-workflow call that passes every secret.
fn inherit(run: &mut Run<'_, Options>, workflow: &Workflow, job: &Node, id: &str) {
    let Some(secrets) = job.get("secrets") else {
        return;
    };
    if secrets.scalar() != Some("inherit") {
        return;
    }
    run.report()
        .finding(
            "inherit",
            Location::line(&workflow.path, secrets.line),
            format!("job `{id}` passes every secret of the repository to the workflow it calls"),
        )
        .key(id)
        .target(format!("{}:{id}", workflow.path))
        .help(
            "list under `secrets:` only the secrets the called workflow declares in \
             `on.workflow_call.secrets`",
        );
}

//! `ci/checkout-credentials`: every checkout leaves no credentials in the clone.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{default_workflows, load, steps};
use gate_sdk::yaml::Node;

/// The options of `ci/checkout-credentials`.
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
    /// The checkout actions, `owner/repo` without a ref.
    pub actions: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            workflows: default_workflows(),
            actions: vec!["actions/checkout".to_owned()],
        }
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct CheckoutCredentials;

impl Rule for CheckoutCredentials {
    type Options = Options;

    const NAME: &'static str = "checkout-credentials";
    const TITLE: &'static str = "Every checkout sets persist-credentials: false";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;
        let checkouts = run.options().actions.clone();

        for workflow in &workflows {
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                for (index, step) in steps(job).iter().enumerate() {
                    let Some(action) = step
                        .get("uses")
                        .and_then(Node::scalar)
                        .and_then(|uses| uses.split_once('@'))
                        .map(|(action, _)| action)
                    else {
                        continue;
                    };
                    if !checkouts.iter().any(|name| name == action) {
                        continue;
                    }
                    let persisted = step
                        .get("with")
                        .and_then(|with| with.get("persist-credentials"))
                        .and_then(Node::scalar);
                    if persisted == Some("false") {
                        continue;
                    }
                    run.report()
                        .finding(
                            "",
                            Location::line(&workflow.path, step.line),
                            format!("job `{id}` checks out with its credentials kept in the clone"),
                        )
                        .key(format!("{id}:{}", index + 1))
                        .target(format!("{}:{id}", workflow.path))
                        .help(
                            "add `with: { persist-credentials: false }`: a token left in the \
                             clone's git configuration reaches every later step; a job that must \
                             push is an exception with its reason",
                        );
                }
            }
        }

        Ok(())
    }
}

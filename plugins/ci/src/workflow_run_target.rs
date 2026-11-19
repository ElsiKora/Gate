//! `ci/workflow-run-target`: every workflow a `workflow_run` trigger names exists.

use std::collections::BTreeSet;

use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{Options, Workflow, load};

/// The rule.
#[derive(Debug, Default)]
pub struct WorkflowRunTarget;

impl Rule for WorkflowRunTarget {
    type Options = Options;

    const NAME: &'static str = "workflow-run-target";
    const TITLE: &'static str = "workflow_run triggers name workflows that exist";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), run.options())?;
        let names: BTreeSet<&str> = workflows.iter().map(Workflow::name).collect();

        for workflow in &workflows {
            let Some(trigger) = workflow
                .root
                .get("on")
                .and_then(|on| on.get("workflow_run"))
            else {
                continue;
            };
            let Some(listed) = trigger.get("workflows") else {
                run.report()
                    .finding(
                        "",
                        Location::line(&workflow.path, trigger.line),
                        "workflow_run names no workflows",
                    )
                    .target(workflow.path.clone())
                    .help(
                        "list the workflows whose runs start this one under workflow_run.workflows",
                    );
                continue;
            };
            let items = listed
                .sequence()
                .map_or_else(|| vec![listed], |items| items.iter().collect());
            for item in items {
                let name = item.scalar().unwrap_or("");
                if names.contains(name) {
                    continue;
                }
                run.report()
                    .finding(
                        "",
                        Location::line(&workflow.path, item.line),
                        format!("workflow_run names `{name}`, which no workflow defines"),
                    )
                    .target(format!("{}:{name}", workflow.path))
                    .help(
                        "name a workflow by its `name:` (or its path when it has none); GitHub \
                         never starts this workflow for a name that no workflow has",
                    );
            }
        }

        Ok(())
    }
}

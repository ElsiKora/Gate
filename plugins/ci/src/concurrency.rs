//! `ci/concurrency`: every workflow has a concurrency group, and only a workflow that runs for pull
//! requests alone cancels a run in progress unconditionally.

use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{Options, Workflow, load};
use gate_sdk::yaml::Node;

/// The triggers whose runs a newer run of the same group may cancel: a pull request's older commits
/// no longer matter.
const CANCELLABLE: &[&str] = &["pull_request", "pull_request_target", "merge_group"];

/// The rule.
#[derive(Debug, Default)]
pub struct Concurrency;

impl Rule for Concurrency {
    type Options = Options;

    const NAME: &'static str = "concurrency";
    const TITLE: &'static str =
        "Every workflow has a concurrency group, and a newer run cancels only a pull request's";
    const ASPECTS: &'static [&'static str] = &["missing", "cancel"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;

        for workflow in &workflows {
            let concurrency = workflow.root.get("concurrency");
            let grouped = concurrency.is_some_and(|node| {
                node.scalar().is_some_and(|group| !group.trim().is_empty())
                    || node
                        .get("group")
                        .and_then(Node::scalar)
                        .is_some_and(|group| !group.trim().is_empty())
            });
            // A workflow only ever called by another runs in its caller's group, and a group of its
            // own named after `github.workflow` (the caller's name) would wait on the caller itself.
            let triggered = triggers(workflow);
            let called_only =
                !triggered.is_empty() && triggered.iter().all(|event| event == "workflow_call");
            if !grouped && !called_only {
                let line = concurrency.map_or(1, |node| node.line);
                run.report()
                    .finding(
                        "missing",
                        Location::line(&workflow.path, line),
                        "the workflow has no concurrency group",
                    )
                    .target(workflow.path.clone())
                    .help(
                        "add a top-level `concurrency:` with a `group` that names the workflow and \
                         the ref (`${{ github.workflow }}-${{ github.ref }}`), so two runs of one \
                         ref queue instead of racing",
                    );
            }
            let Some(cancel) = concurrency.and_then(|node| node.get("cancel-in-progress")) else {
                continue;
            };
            if cancel.scalar() != Some("true") {
                continue;
            }
            let others: Vec<String> = triggered
                .into_iter()
                .filter(|trigger| !CANCELLABLE.contains(&trigger.as_str()))
                .collect();
            if others.is_empty() {
                continue;
            }
            run.report()
                .finding(
                    "cancel",
                    Location::line(&workflow.path, cancel.line),
                    format!(
                        "cancel-in-progress is true, so a newer run also cancels a running `{}` run",
                        others.join("`, `")
                    ),
                )
                .target(workflow.path.clone())
                .help(
                    "cancel only pull-request runs: `cancel-in-progress: ${{ github.event_name == \
                     'pull_request' }}`, so a push to the default branch, a deploy or a publish \
                     always runs to its end",
                );
        }

        Ok(())
    }
}

/// The events of the workflow's `on:`, whether it is one event, a list or a mapping.
fn triggers(workflow: &Workflow) -> Vec<String> {
    let Some(on) = workflow.root.get("on") else {
        return Vec::new();
    };
    if let Some(event) = on.scalar() {
        return vec![event.to_owned()];
    }
    if let Some(events) = on.sequence() {
        return events
            .iter()
            .filter_map(Node::scalar)
            .map(str::to_owned)
            .collect();
    }
    on.mapping()
        .unwrap_or_default()
        .iter()
        .filter_map(|(event, _)| event.scalar())
        .map(str::to_owned)
        .collect()
}

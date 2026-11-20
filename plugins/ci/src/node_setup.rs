//! `ci/node-setup`: `actions/setup-node` reads the version from a file, caches the package
//! manager's downloads, and runs once per job.

use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{Options, Workflow, is_expression, load, steps};
use gate_sdk::yaml::Node;

/// The action the rule reads.
const SETUP_NODE: &str = "actions/setup-node";

/// The rule.
#[derive(Debug, Default)]
pub struct NodeSetup;

impl Rule for NodeSetup {
    type Options = Options;

    const NAME: &'static str = "node-setup";
    const TITLE: &'static str =
        "actions/setup-node reads the version from a file, sets a cache and runs once per job";
    const ASPECTS: &'static [&'static str] = &["version-file", "cache", "once"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;

        for workflow in &workflows {
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                let setups: Vec<&Node> = steps(job)
                    .iter()
                    .filter(|step| {
                        step.get("uses")
                            .and_then(Node::scalar)
                            .and_then(|uses| uses.split_once('@'))
                            .is_some_and(|(action, _)| action == SETUP_NODE)
                    })
                    .collect();
                for (index, step) in setups.iter().enumerate() {
                    check_step(run, workflow, id, step, index);
                }
            }
        }

        Ok(())
    }
}

/// Checks the `index`th (from 0) setup-node step of job `id`.
fn check_step(
    run: &mut Run<'_, Options>,
    workflow: &Workflow,
    id: &str,
    step: &Node,
    index: usize,
) {
    let target = format!("{}:{id}", workflow.path);
    let at = Location::line(&workflow.path, step.line);
    if index > 0 {
        run.report()
            .finding("once", at.clone(), format!("job `{id}` sets up Node again"))
            .key(format!("{id}:{}", index + 1))
            .target(target.clone())
            .help(
                "set up Node once per job, with the package manager's `cache` in that one step; \
                 install the package manager before it if `cache` needs it",
            );
    }
    let with = step.get("with");
    let input = |name: &str| with.and_then(|with| with.get(name));
    // A version the workflow computes (a matrix entry, an output) is chosen on purpose, not typed in.
    let computed = input("node-version")
        .and_then(Node::scalar)
        .is_some_and(is_expression);
    if input("node-version-file").is_none() && !computed {
        run.report()
            .finding(
                "version-file",
                at.clone(),
                format!("job `{id}` sets up Node without node-version-file"),
            )
            .key(format!("{id}:{}", index + 1))
            .target(target.clone())
            .help(
                "read the version from the repository (`node-version-file: .nvmrc` or \
                 `package.json`), so every workflow and every developer use the same Node",
            );
    }
    if input("cache").is_none() {
        run.report()
            .finding(
                "cache",
                at,
                format!("job `{id}` sets up Node without a cache"),
            )
            .key(format!("{id}:{}", index + 1))
            .target(target)
            .help("set `cache` to the package manager (`npm`, `pnpm`, `yarn`)");
    }
}

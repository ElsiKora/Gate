//! `ci/actions-pinned`: every action and reusable workflow a workflow uses is pinned by a full
//! commit SHA, or a Docker image by its digest.

use std::sync::LazyLock;

use gate_sdk::{Location, Rule, Run, Scope};
use regex::Regex;

use crate::workflows::{Options, load};
use gate_sdk::yaml::Node;

/// `owner/repo[/path]@<40 lowercase hex>`.
static COMMIT: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^[^@\s]+@[0-9a-f]{40}$").ok());
/// `docker://image@sha256:<64 lowercase hex>`.
static DIGEST: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^docker://[^@\s]+@sha256:[0-9a-f]{64}$").ok());

/// The rule.
#[derive(Debug, Default)]
pub struct ActionsPinned;

impl Rule for ActionsPinned {
    type Options = Options;

    const NAME: &'static str = "actions-pinned";
    const TITLE: &'static str =
        "Workflows use actions and reusable workflows pinned by a full commit SHA";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let (Some(commit), Some(digest)) = (COMMIT.as_ref(), DIGEST.as_ref()) else {
            return Err(gate_sdk::Error::new(
                "the pin patterns of ci/actions-pinned do not compile; report this as a bug",
            ));
        };
        let workflows = load(&run.tree(), run.options())?;

        for workflow in &workflows {
            for job in workflow.jobs() {
                let steps = job
                    .get("steps")
                    .and_then(Node::sequence)
                    .unwrap_or_default();
                let uses = job
                    .get("uses")
                    .into_iter()
                    .chain(steps.iter().filter_map(|step| step.get("uses")));
                for node in uses {
                    let reference = node.scalar().unwrap_or("");
                    if reference.starts_with("./")
                        || commit.is_match(reference)
                        || digest.is_match(reference)
                    {
                        continue;
                    }
                    run.report()
                        .finding(
                            "",
                            Location::line(&workflow.path, node.line),
                            format!("`{reference}` is not pinned by a full commit SHA"),
                        )
                        .target(format!("{}:{reference}", workflow.path))
                        .help(
                            "pin the action to the full 40-character commit SHA of the release \
                             you mean, with the tag in a comment (`uses: owner/action@<sha> # v4`), \
                             or a Docker image to its sha256 digest",
                        );
                }
            }
        }

        Ok(())
    }
}

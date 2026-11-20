//! `ci/actions-pinned`: every action and reusable workflow a workflow uses is pinned by a full
//! commit SHA, or a Docker image by its digest, and a commit pin names its release in a comment.

use std::sync::LazyLock;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Rule, Run, Scope};
use regex::Regex;

use crate::workflows::{Workflow, default_workflows, load, steps};
use gate_sdk::yaml::Node;

/// `owner/repo[/path]@<40 lowercase hex>`.
static COMMIT: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^[^@\s]+@[0-9a-f]{40}$").ok());
/// `docker://image@sha256:<64 lowercase hex>`.
static DIGEST: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^docker://[^@\s]+@sha256:[0-9a-f]{64}$").ok());

/// The options of `ci/actions-pinned`.
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
    /// A regular expression the comment after a commit pin must match: the release the commit is
    /// (`# v4.2.2`), without the `#`.
    pub version_comment: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            workflows: default_workflows(),
            version_comment: r"^v\d+\.\d+\.\d+$".to_owned(),
        }
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct ActionsPinned;

impl Rule for ActionsPinned {
    type Options = Options;

    const NAME: &'static str = "actions-pinned";
    const TITLE: &'static str = "Workflows use actions and reusable workflows pinned by a full commit SHA, with the release in a comment";
    const ASPECTS: &'static [&'static str] = &["version-comment"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let (Some(commit), Some(digest)) = (COMMIT.as_ref(), DIGEST.as_ref()) else {
            return Err(Error::new(
                "the pin patterns of ci/actions-pinned do not compile; report this as a bug",
            ));
        };
        let version = Regex::new(&run.options().version_comment).map_err(|error| {
            Error::config(format!(
                "version-comment is not a valid regular expression ({error}); write one such as \
                 \"^v\\\\d+\\\\.\\\\d+\\\\.\\\\d+$\""
            ))
        })?;
        let workflows = load(&run.tree(), &run.options().workflows)?;

        for workflow in &workflows {
            for job in workflow.jobs() {
                let uses = job
                    .get("uses")
                    .into_iter()
                    .chain(steps(job).iter().filter_map(|step| step.get("uses")));
                for node in uses {
                    let reference = node.scalar().unwrap_or("");
                    if reference.starts_with("./") || digest.is_match(reference) {
                        continue;
                    }
                    if commit.is_match(reference) {
                        let comment = comment(workflow, node, reference);
                        if comment.is_none_or(|comment| !version.is_match(comment)) {
                            report_comment(run, workflow, node, reference, comment);
                        }
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
                             you mean, with the release in a comment \
                             (`uses: owner/action@<sha> # v4.2.2`), or a Docker image to its \
                             sha256 digest",
                        );
                }
            }
        }

        Ok(())
    }
}

/// The comment after `reference` on the line of `node`, without the `#`, trimmed; none when the
/// line has no comment there.
fn comment<'a>(workflow: &'a Workflow, node: &Node, reference: &str) -> Option<&'a str> {
    let line = workflow.line(node.line);
    let at = line.find(reference)?;
    let rest = line[at + reference.len()..]
        .trim_start_matches(['"', '\''])
        .trim_start();
    rest.strip_prefix('#').map(str::trim)
}

fn report_comment(
    run: &mut Run<'_, Options>,
    workflow: &Workflow,
    node: &Node,
    reference: &str,
    comment: Option<&str>,
) {
    let message = match comment {
        Some(comment) => format!(
            "`{reference}` names its release as `{comment}`, which does not match \
             version-comment"
        ),
        None => format!("`{reference}` has no comment naming its release"),
    };
    run.report()
        .finding(
            "version-comment",
            Location::line(&workflow.path, node.line),
            message,
        )
        .key(reference)
        .target(format!("{}:{reference}", workflow.path))
        .help(
            "write the full release the commit is after the pin, `uses: owner/action@<sha> # \
             v4.2.2`, so a reviewer sees which release the SHA is and a dependency bot can update \
             both",
        );
}

//! `agents/rule-owner`: every rule file names an accepted decision of the decision index as its
//! owner.

use std::collections::BTreeMap;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Result, Rule, Run};

use crate::load::{Agents, at, default_root, strict_text};

/// The rule.
#[derive(Debug, Default)]
pub struct RuleOwner;

/// The options of `agents/rule-owner`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directory of the agent instructions, relative to the project.
    pub root: String,
    /// The decision index: a Markdown table whose rows start with a backticked `D-` id and
    /// whose second cell is the status.
    pub decisions: String,
    /// The status of a decision that may own a rule file.
    pub accepted_status: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: default_root(),
            decisions: "docs/ledger/decision-index.md".to_owned(),
            accepted_status: "accepted".to_owned(),
        }
    }
}

impl Rule for RuleOwner {
    type Options = Options;

    const NAME: &'static str = "rule-owner";
    const TITLE: &'static str = "Every rule file owner is an accepted decision";

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let tree = run.tree();
        let options = run.options();
        let agents = Agents::load(&tree, &options.root)?;
        if agents.rules.is_empty() {
            return Ok(());
        }
        let index = strict_text(&tree, &options.decisions)?.map(|text| statuses(&text));
        let accepted = |owner: &str| {
            index
                .as_ref()
                .and_then(|index| index.get(owner))
                .is_some_and(|status| *status == options.accepted_status)
        };
        let mut found = Vec::new();

        for file in &agents.rules {
            if file.frontmatter.is_none() {
                continue;
            }
            let line = file.field("owner").map(|node| node.line).or(Some(1));
            let message = match file.owner() {
                None => "frontmatter has no owner decision".to_owned(),
                Some(owner) if accepted(owner) => continue,
                Some(owner) => {
                    format!("owner {owner} is not an accepted decision in the decision index")
                }
            };
            found.push((at(&file.path, line), file.path.clone(), message));
        }

        for (location, target, message) in found {
            run.report()
                .finding("", location, message)
                .target(target)
                .help("set `owner` to the id of an accepted decision in the decision index");
        }
        Ok(())
    }
}

/// The status of every decision in the index; the first row of an id wins.
fn statuses(text: &str) -> BTreeMap<String, String> {
    let mut statuses = BTreeMap::new();
    for line in text.lines().filter(|line| line.starts_with("| `D-")) {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
        if let [id, status, _, _, ..] = cells.as_slice() {
            statuses
                .entry(unquote(id))
                .or_insert_with(|| unquote(status));
        }
    }
    statuses
}

/// The first backticked token of a cell, or the trimmed cell.
fn unquote(cell: &str) -> String {
    let cell = cell.trim();
    if let Some(rest) = cell.strip_prefix('`')
        && let Some(end) = rest.find('`')
    {
        return rest[..end].to_owned();
    }
    cell.to_owned()
}

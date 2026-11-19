//! `ledger/decision-counts`: the number of decision rows and of superseded decisions, in the managed
//! block `decision-counts` of the decision index.

use std::collections::HashMap;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Generate, Generator, Result};

use super::write_block;
use crate::settings::check_path;

/// The name of the managed block.
pub const BLOCK: &str = "decision-counts";

/// The generator.
#[derive(Debug, Default)]
pub struct DecisionCounts;

/// The options of `ledger/decision-counts`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The decision index that holds the block, relative to the project; nothing is generated while
    /// it is not tracked.
    pub decisions: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            decisions: default_decisions(),
        }
    }
}

/// The decision index of `[plugins.ledger] options`, by default.
pub(crate) fn default_decisions() -> String {
    "docs/ledger/decision-index.md".to_owned()
}

impl Generator for DecisionCounts {
    type Options = Options;

    const NAME: &'static str = "decision-counts";
    const TITLE: &'static str =
        "The decision and superseded counts in the managed block of the decision index";
    /// It writes only the decision index.
    const OWNS: &'static [&'static str] = &[];

    fn generate(&self, run: &mut Generate<'_, Options>) -> Result<()> {
        let path = run.options().decisions.clone();
        check_path("[generate] ledger/decision-counts decisions", &path)?;
        let Some(text) = run.tree().text(&path) else {
            return Ok(());
        };
        let (rows, superseded) = counts(&text);
        write_block(run, &path, &text, BLOCK, &render(rows, superseded));
        Ok(())
    }
}

/// The rows of the index table (lines that start with ``| `D-``, repeats included) and the
/// decisions whose status starts with `superseded`, each counted at its first row with four cells.
#[must_use]
pub fn counts(text: &str) -> (usize, usize) {
    let mut rows = 0;
    let mut statuses: HashMap<String, String> = HashMap::new();

    for line in text.lines() {
        if !line.starts_with("| `D-") {
            continue;
        }
        rows += 1;
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
        if let [id, status, _, _, ..] = cells.as_slice() {
            statuses
                .entry(unquote(id))
                .or_insert_with(|| unquote(status));
        }
    }

    let superseded = statuses
        .values()
        .filter(|status| status.starts_with("superseded"))
        .count();
    (rows, superseded)
}

/// The first backticked token of a cell, or the trimmed cell.
fn unquote(cell: &str) -> String {
    let cell = cell.trim();
    cell.strip_prefix('`')
        .and_then(|rest| rest.find('`').map(|end| &rest[..end]))
        .unwrap_or(cell)
        .to_owned()
}

/// The text of the block.
#[must_use]
pub fn render(rows: usize, superseded: usize) -> String {
    format!("- Decision IDs: `{rows}`\n- Superseded decisions: `{superseded}`\n")
}

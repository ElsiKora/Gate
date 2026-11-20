//! Decision tables: the decision index and the ADR coverage table, Markdown tables whose rows
//! start with a backticked `D-` identifier, then the status, a middle cell and the canonical file.
//! One parser reads both, for every rule.

use std::collections::BTreeMap;

use gate_sdk::Tree;
use regex::Regex;

use crate::records::compile;
use crate::text::{blank_fenced, trim};

/// One well-formed row of a decision table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    /// The identifier, without backticks.
    pub id: String,
    /// The status, without backticks.
    pub status: String,
    /// The middle cell as written: the title in the index, the ADR references in the coverage.
    pub middle: String,
    /// The canonical file, without backticks.
    pub canonical: String,
    /// The 1-based line.
    pub line: u32,
}

/// A line of a decision table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Line {
    /// A well-formed row.
    Row(Row),
    /// A line that starts a row but is not one: its line number and text.
    Malformed(u32, String),
}

/// The patterns of a decision table.
#[derive(Debug)]
pub struct Table {
    row: Regex,
    id_cell: Regex,
    quoted_cell: Regex,
}

impl Table {
    /// The patterns, compiled.
    ///
    /// # Errors
    /// Never in practice: the patterns are constants.
    pub fn new() -> gate_sdk::Result<Self> {
        Ok(Self {
            row: compile(r"^\|\s*`D-")?,
            id_cell: compile(r"^`D-[^`]+`$")?,
            quoted_cell: compile(r"^`[^`]+`$")?,
        })
    }

    /// The rows of `text`: the lines outside fenced code that start with `|` and a backticked `D-`
    /// identifier. A row has exactly four cells, each a backticked value except the middle one,
    /// which `middle` accepts; any other such line is malformed.
    #[must_use]
    pub fn lines(&self, text: &str, middle: impl Fn(&str) -> bool) -> Vec<Line> {
        let mut lines = Vec::new();

        for (index, line) in blank_fenced(text, true).split('\n').enumerate() {
            if !self.row.is_match(line) {
                continue;
            }

            let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
            let parts: Vec<&str> = line.split('|').collect();
            let cells: Vec<&str> = parts
                .get(1..parts.len().saturating_sub(1))
                .unwrap_or_default()
                .iter()
                .map(|cell| trim(cell))
                .collect();
            let row = match cells.as_slice() {
                [id, status, between, canonical]
                    if self.id_cell.is_match(id)
                        && self.quoted_cell.is_match(status)
                        && self.quoted_cell.is_match(canonical)
                        && middle(between) =>
                {
                    Line::Row(Row {
                        id: unquote(id),
                        status: unquote(status),
                        middle: (*between).to_owned(),
                        canonical: unquote(canonical),
                        line: number,
                    })
                }
                _ => Line::Malformed(number, line.to_owned()),
            };
            lines.push(row);
        }

        lines
    }
}

/// A cell without its enclosing backticks; the cell matched a backticked pattern.
fn unquote(cell: &str) -> String {
    cell.get(1..cell.len().saturating_sub(1))
        .unwrap_or_default()
        .to_owned()
}

/// Why the decision a relaxation names does not justify it.
#[derive(Debug, Eq, PartialEq)]
pub enum Refusal<'i> {
    /// The index does not list it, or there is no index.
    Unlisted,
    /// The index lists it with this status, which is not `accepted`.
    NotAccepted(&'i str),
}

/// Why the decision `id` does not justify a relaxation, by `index` (`None` when the index is not
/// tracked); `None` when the index lists it as accepted. `ledger/exception-decisions` and
/// `ledger/rule-decisions` judge alike through it.
#[must_use]
pub fn refusal<'i>(index: Option<&'i DecisionIndex>, id: &str) -> Option<Refusal<'i>> {
    match index.and_then(|index| index.get(id)) {
        None => Some(Refusal::Unlisted),
        Some(row) if row.status != "accepted" => Some(Refusal::NotAccepted(&row.status)),
        Some(_) => None,
    }
}

/// The decisions of the index by identifier, read by [`Table`]; a later row of an identifier
/// replaces an earlier one, as `ledger/decisions` reads it (and reports the repeat).
#[derive(Debug)]
pub struct DecisionIndex {
    decisions: BTreeMap<String, Row>,
}

impl DecisionIndex {
    /// The decision with identifier `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Row> {
        self.decisions.get(id)
    }

    /// Whether the index lists `id` with the status `accepted`.
    #[must_use]
    pub fn is_accepted(&self, id: &str) -> bool {
        self.get(id).is_some_and(|row| row.status == "accepted")
    }

    /// Reads the index at `path`; `None` when it is not tracked.
    ///
    /// # Errors
    /// Never in practice: see [`Table::new`].
    pub fn load(tree: &Tree<'_>, path: &str) -> gate_sdk::Result<Option<Self>> {
        let Some(text) = tree.text(path) else {
            return Ok(None);
        };
        Ok(Some(parse(&Table::new()?, &text)))
    }
}

/// The index of `text`: its well-formed rows, whose title is not empty.
#[must_use]
pub fn parse(table: &Table, text: &str) -> DecisionIndex {
    let decisions = table
        .lines(text, |title| !title.is_empty())
        .into_iter()
        .filter_map(|line| match line {
            Line::Row(row) => Some((row.id.clone(), row)),
            Line::Malformed(..) => None,
        })
        .collect();
    DecisionIndex { decisions }
}

#[cfg(test)]
mod tests {
    use super::{Table, parse};

    #[test]
    fn rows_have_four_cells_outside_fenced_code_and_a_later_row_replaces_an_earlier_one() {
        let table = Table::new().expect("patterns");
        let index = parse(
            &table,
            "| `D-A-001` | `proposed` | A | `a.md` |\n| `D-A-001` | `accepted` | A | `a.md` |\n| `D-B-001` | `accepted` |\n| `D-C-001` | `accepted` | C | `c.md` | extra |\n```\n| `D-D-001` | `accepted` | D | `d.md` |\n```\n",
        );
        assert!(index.is_accepted("D-A-001"));
        assert_eq!(index.get("D-A-001").map(|row| row.line), Some(2));
        assert!(index.get("D-B-001").is_none(), "three cells");
        assert!(index.get("D-C-001").is_none(), "five cells");
        assert!(index.get("D-D-001").is_none(), "inside fenced code");
    }
}

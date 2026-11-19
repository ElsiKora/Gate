//! The accepted decisions of a Markdown decision index, which rules that cite decisions read.

use std::collections::BTreeSet;

/// The identifiers of the accepted decisions of a Markdown decision index: rows that start with
/// ``| `D-`` and have at least four cells, the identifier in the first and the status in the
/// second; the first row of an identifier counts.
#[must_use]
pub fn accepted(text: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut accepted = BTreeSet::new();

    for line in text.lines().filter(|line| line.starts_with("| `D-")) {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();

        if cells.len() < 4 {
            continue;
        }

        let id = unquote(cells[0]);

        if seen.insert(id.clone()) && unquote(cells[1]) == "accepted" {
            accepted.insert(id);
        }
    }

    accepted
}

/// The first backticked token of a cell, or the trimmed cell.
fn unquote(cell: &str) -> String {
    let cell = cell.trim();

    cell.strip_prefix('`')
        .and_then(|rest| rest.find('`').map(|end| rest[..end].to_owned()))
        .unwrap_or_else(|| cell.to_owned())
}

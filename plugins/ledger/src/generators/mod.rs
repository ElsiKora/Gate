//! The generators of the ledger plugin: counts written into managed blocks of hand-written ledger
//! documents. Each writes only the document it names, so neither owns a glob.

pub mod decision_counts;
pub mod review_progress;

use gate_sdk::{Generate, Location};

use crate::block;

/// The location of a one-based `line` of `path`, or the whole file.
fn at(path: &str, line: Option<usize>) -> Location {
    match line {
        Some(line) => Location::line(path.to_owned(), line),
        None => Location::file(path.to_owned()),
    }
}

/// Writes `text` with the block `name` holding `inner`; markers that are missing, repeated or
/// reversed are a finding at the first marker's line, and nothing is written.
fn write_block<O>(run: &mut Generate<'_, O>, path: &str, text: &str, name: &str, inner: &str) {
    match block::replace(text, name, inner) {
        Ok(contents) => run.write(path.to_owned(), contents),
        Err((line, message)) => {
            run.report()
                .finding("", at(path, line), message)
                .target(path.to_owned())
                .help(format!(
                    "put one `<!-- gate:begin {name} -->` line and, below it, one \
                     `<!-- gate:end {name} -->` line in {path}"
                ));
        }
    }
}

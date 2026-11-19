//! The generator `docs/layout`: seeds the index of every required entry of the project's kind.
//! `gate sync` writes a seed only where nothing exists, so a page a person filled is never
//! overwritten, and a required page deleted on purpose comes back.

use std::fmt::Write as _;

use gate_sdk::{Generate, Generator, NoOptions, Result};

use crate::settings::Settings;
use crate::standard::{self, Entry, Index, Kind, SEED_MARKER};

/// The generator.
#[derive(Debug, Default)]
pub struct LayDown;

impl Generator for LayDown {
    type Options = NoOptions;

    const NAME: &'static str = "layout";
    const TITLE: &'static str =
        "Seeds the required index pages of the project's documentation standard";

    fn generate(&self, run: &mut Generate<'_, NoOptions>) -> Result<()> {
        let settings = Settings::read(run.plugin_options())?;
        let kind = settings.kind;
        for (entry, index) in standard::required_indexes(kind) {
            run.seed(entry.index(), page(kind, entry, index));
        }
        Ok(())
    }
}

/// The seed of `index` in `kind`: its title, the marker line, and each required section with the
/// line that says what goes there. The book's index adds the table of the seeded entries; the
/// decision log adds the record template.
#[must_use]
pub fn page(kind: Kind, entry: &Entry, index: &Index) -> String {
    let mut text = format!("# {}\n\n{SEED_MARKER}\n", index.title);
    for section in index.sections[kind.column()] {
        let _ = write!(text, "\n## {}\n\n{}\n", section.title, section.says);
        match (entry.name, section.title) {
            ("README.md", "Entries") => text.push_str(&entry_table(kind)),
            ("adr", "Template") => text.push_str(ADR_TEMPLATE),
            _ => {}
        }
    }
    text
}

/// The table of the entries `kind` seeds, in the standard order, each linked from `docs/`.
fn entry_table(kind: Kind) -> String {
    let mut table = String::from("\n| Entry | Job |\n| --- | --- |\n");
    for (entry, _) in standard::required_indexes(kind) {
        if entry.name == "README.md" {
            continue;
        }
        let (label, link) = if entry.directory {
            (
                format!("{}/", entry.name),
                format!("{}/README.md", entry.name),
            )
        } else {
            (entry.name.to_owned(), entry.name.to_owned())
        };
        let _ = writeln!(table, "| [{label}]({link}) | {} |", entry.job);
    }
    table
}

const ADR_TEMPLATE: &str = "
```markdown
# NNNN: The decision, in a few words

| Field | Value |
| --- | --- |
| Status | proposed |
| Date | YYYY-MM-DD |

## Context

## Decision

## Consequences
```
";

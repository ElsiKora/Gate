//! `nestjs/entity-table-name`: every entity decorator names its table with a double-quoted string
//! as its first argument.
//!
//! The scan is textual, as the regular expression `/@Entity\s*\(([^)]*)\)/g` reads a file: every
//! tracked `.ts` file below `layout.source-root` is read (declaration files and every folder
//! included, nothing skipped), as UTF-8 with invalid bytes replaced; comments and strings count, the
//! import source is not checked, and the arguments end at the first `)`. A match resumes the
//! search after itself, and two matches on one line are two findings, told apart by their column.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::javascript::is_space;

use crate::layout::PluginOptions;
use crate::shared::tree;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/entity-table-name`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The decorator text, `@` included, whose first argument must name the table.
    pub decorator: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            decorator: "@Entity".to_owned(),
        }
    }
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "entity-table-name";
    const TITLE: &'static str = "Every @Entity names its table with a double-quoted string first";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let decorator = run.options().decorator.clone();

            if decorator.is_empty() {
                return Err(gate_sdk::Error::config(
                    "the option decorator is empty; name the decorator, for example \"@Entity\"",
                ));
            }

            let source = shared.source();
            let paths: Vec<&str> =
                tree::typescript_files(source, &layout.source_root, &tree::UNSKIPPED).collect();

            for path in paths {
                let Some(bytes) = shared.bytes(path) else {
                    continue;
                };
                // Node decodes a file that is not UTF-8 with replacement characters and still reads it.
                let text = String::from_utf8_lossy(&bytes);

                for (line, column) in unnamed(&text, &decorator) {
                    run.report()
                        .finding(
                            "",
                            Location::line(path, line),
                            format!(
                                "{path} uses {decorator}() at column {column} without an explicit double-quoted table name; use {decorator}(\"table_name\")"
                            ),
                        )
                        .help(format!(
                            "name the table first, in double quotes: {decorator}(\"table_name\")"
                        ))
                        .key(decorator.as_str());
                }
            }

            Ok(())
        })
    }
}

/// Line and column (one-based, the column in characters) of every `decorator(...)` whose first
/// argument is not a double-quoted table name.
fn unnamed(text: &str, decorator: &str) -> Vec<(usize, usize)> {
    let mut findings = Vec::new();
    let mut from = 0;

    while let Some(offset) = text[from..].find(decorator) {
        let start = from + offset;
        let rest = &text[start + decorator.len()..];
        let opening = rest.trim_start_matches(is_space);
        let arguments = opening
            .strip_prefix('(')
            .and_then(|inside| inside.find(')').map(|close| (inside, close)));

        let Some((inside, close)) = arguments else {
            from = start + decorator.chars().next().map_or(1, char::len_utf8);
            continue;
        };

        if !names_table(inside[..close].trim_matches(is_space)) {
            let before = &text[..start];
            let line = before.matches('\n').count() + 1;
            let column = before[before.rfind('\n').map_or(0, |newline| newline + 1)..]
                .chars()
                .count()
                + 1;
            findings.push((line, column));
        }

        from = start + decorator.len() + (rest.len() - opening.len()) + 1 + close + 1;
    }

    findings
}

/// `"name"` with at least one character, optionally followed by more arguments after a comma.
fn names_table(arguments: &str) -> bool {
    let Some(quoted) = arguments.strip_prefix('"') else {
        return false;
    };
    let Some(close) = quoted.find('"') else {
        return false;
    };
    let after = quoted[close + 1..].trim_start_matches(is_space);

    close > 0 && (after.is_empty() || after.starts_with(','))
}

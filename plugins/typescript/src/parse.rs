//! `typescript/parse`: every tracked TypeScript or JavaScript source file can be parsed into a
//! syntax tree.
//!
//! oxc returns an empty program for a file it cannot recover from, and the library does the same
//! for a file that is not UTF-8 text; a rule reading the syntax tree would take either for a clean
//! file. This rule reports each such file once, at its first syntax error, and the rules that read
//! the syntax tree skip it. Errors oxc recovers from are not reported: the TypeScript compiler and
//! `ESLint` own them. The rule reads no tsconfig.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run, Span};
use gate_typescript::{Allocator, ReadSource, SOURCE_EXTENSIONS, Settings, TypeScript};

/// The rule.
#[derive(Debug, Default)]
pub struct Parse;

/// The options of `typescript/parse`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The extensions (without the dot) of the files that must parse. They compare
    /// case-sensitively, as the TypeScript compiler reads them, so `App.TS` is not a source file.
    /// Each must be one oxc parses: `ts`, `tsx`, `mts`, `cts`, `js`, `jsx`, `mjs` or `cjs`.
    pub extensions: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            extensions: SOURCE_EXTENSIONS.map(str::to_owned).to_vec(),
        }
    }
}

impl Rule for Parse {
    type Options = Options;

    const NAME: &'static str = "parse";
    const TITLE: &'static str =
        "Every tracked TypeScript or JavaScript source file can be parsed into a syntax tree";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        if let Some(unknown) = run
            .options()
            .extensions
            .iter()
            .find(|extension| !SOURCE_EXTENSIONS.contains(&extension.as_str()))
        {
            return Err(gate_sdk::Error::config(format!(
                "extensions names `{unknown}`, which oxc does not parse; use only {}",
                SOURCE_EXTENSIONS.join(", ")
            )));
        }

        let tree = run.tree();
        let extensions: Vec<&str> = run
            .options()
            .extensions
            .iter()
            .map(String::as_str)
            .collect();
        let paths = tree.with_extensions(&extensions);
        let source = ReadSource::new(paths.clone(), move |path| tree.bytes(path));
        // `parse` reads no tsconfig, so the settings do not change what it reports; they are read
        // so that a mistake in `[languages.typescript]` fails the run.
        let settings = run.language::<Settings>("typescript")?.unwrap_or_default();
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, settings, &allocator);

        for path in &paths {
            let file = typescript
                .parse(path)
                .map_err(|error| gate_sdk::Error::new(error.to_string()))?
                .ok_or_else(|| {
                    gate_sdk::Error::new(format!(
                        "{path} is tracked but its contents could not be read; check that the \
                         file exists in the snapshot and that the plugin is granted it"
                    ))
                })?;

            if file.fatal() {
                let first = file.errors().first();
                let location = match first.map(|error| (error.line, error.column)) {
                    Some((Some(line), Some(column))) => Location::span(
                        path,
                        Span {
                            line,
                            column,
                            end_line: line,
                            end_column: column,
                        },
                    ),
                    Some((Some(line), None)) => {
                        Location::line(path, usize::try_from(line).unwrap_or(usize::MAX))
                    }
                    _ => Location::file(path),
                };
                run.report()
                    .finding(
                        "",
                        location,
                        format!(
                            "{path} cannot be parsed, so the checks that read its syntax tree skip it: {}",
                            first.map_or("syntax error", |error| error.message.as_str())
                        ),
                    )
                    .target(path.clone())
                    .help(
                        "fix the syntax error so the file parses; if the file is not source code, \
                         rename it or leave its extension out of the extensions option",
                    );
            }
        }

        Ok(())
    }
}

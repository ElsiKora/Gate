//! `agents/rule-file-size`: a rule file stays short enough to load into an agent's context.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Result, Rule, Run};

use crate::load::{Agents, INPUT, default_root};

/// The rule.
#[derive(Debug, Default)]
pub struct RuleFileSize;

/// The options of `agents/rule-file-size`.
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
    /// A file with more lines is reported as `#max-lines`.
    #[schemars(range(min = 1))]
    pub max_lines: usize,
    /// A file with more lines, up to `max-lines`, is reported as `#warn-lines`.
    #[schemars(range(min = 1))]
    pub warn_lines: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: default_root(),
            max_lines: 500,
            warn_lines: 200,
        }
    }
}

impl Rule for RuleFileSize {
    type Options = Options;

    const NAME: &'static str = "rule-file-size";
    const TITLE: &'static str = "A rule file has at most max-lines lines";
    const ASPECTS: &'static [&'static str] = &[INPUT, "max-lines", "warn-lines"];

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let (max, warn) = (run.options().max_lines, run.options().warn_lines);
        if warn > max {
            return Err(Error::config(format!(
                "warn-lines {warn} is above max-lines {max}; set warn-lines at or below max-lines"
            )));
        }
        let agents = Agents::load(&run.tree(), &run.options().root)?;
        if !agents.has_rules(run.report(), "agents/rule-file-size", INPUT) {
            return Ok(());
        }
        for file in &agents.rules {
            let lines = file.line_count;
            let (aspect, message) = if lines > max {
                (
                    "max-lines",
                    format!("{lines} lines exceed the {max}-line limit"),
                )
            } else if lines > warn {
                (
                    "warn-lines",
                    format!("{lines} lines exceed {warn}; consider splitting the area"),
                )
            } else {
                continue;
            };
            run.report()
                .finding(aspect, Location::file(&file.path), message)
                .key("line-count")
                .target(file.path.clone())
                .help("split the area into two rule files, each with its own area");
        }
        Ok(())
    }
}

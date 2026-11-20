//! The aspect `input` every ledger rule that reads a file has: the file is not tracked, so the rule
//! cannot run. A missing input is a finding, never a silent pass; `levels = { input = "off" }` or an
//! exception is the explicit way out.

use gate_sdk::{Location, Report, Tree};

use crate::settings::Settings;

/// The aspect.
pub const INPUT: &str = "input";

/// A file a rule reads, and the option that names it, as a message says it.
#[derive(Debug)]
pub struct Input {
    option: String,
    path: String,
}

impl Input {
    /// A file `[plugins.ledger] options` `key` names.
    #[must_use]
    pub fn shared(key: &str, path: &str) -> Self {
        Self {
            option: format!("[plugins.ledger] options {key}"),
            path: path.to_owned(),
        }
    }

    /// A file the rule's own option `key` names.
    #[must_use]
    pub fn own(key: &str, path: &str) -> Self {
        Self {
            option: format!("its option {key}"),
            path: path.to_owned(),
        }
    }
}

/// The files every rule that reads the records reads: `implementation`, and with `parity` its
/// plan.
#[must_use]
pub fn records(settings: &Settings) -> Vec<Input> {
    let mut inputs = vec![Input::shared("implementation", &settings.implementation)];
    if let Some(parity) = &settings.parity {
        inputs.push(Input::shared("parity.plan", &parity.plan));
    }
    inputs
}

/// Reports each of `inputs` that `tree` does not track as a finding of `ledger/<rule>#input`, at
/// the file and keyed by its path; whether every input is tracked.
pub fn tracked(report: &mut Report, tree: &Tree<'_>, rule: &str, inputs: &[Input]) -> bool {
    let mut all = true;
    for Input { option, path } in inputs {
        if tree.contains(path) {
            continue;
        }
        all = false;
        report
            .finding(
                INPUT,
                Location::file(path.clone()),
                format!(
                    "{path} is not tracked, and ledger/{rule} reads it ({option}); create it, point the option at the file, or turn the input off with a decision: \"ledger/{rule}\" = {{ levels = {{ input = \"off\" }}, decision = \"<record>\" }}"
                ),
            )
            .target(path.clone())
            .key(path.clone());
    }
    all
}

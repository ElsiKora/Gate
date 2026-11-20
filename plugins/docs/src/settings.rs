//! The `[plugins.docs]` options: the project's kind, its declared extensions and the delegated
//! entries whose plugins it enables. The rule and the generator read the same options.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Result};

use crate::standard::{self, Cell, ENTRIES, Kind};

/// The plugin's options.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Settings {
    /// The project's kind: its standard. Required; never inferred.
    pub kind: Kind,
    /// The project's own top-level entries, each with its job, after the standard's in the
    /// index's order, in the order declared.
    #[serde(default)]
    pub extra: Vec<Extra>,
    /// The delegated entries (`agents`, `ledger`) whose owning plugin the project enables: the
    /// index's seed lists each, the index must link each, and each must be tracked.
    #[serde(default)]
    pub delegated: Vec<String>,
}

/// An entry a project adds to its standard.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Extra {
    /// Kebab-case: a directory (with its `README.md`), or a Markdown file ending in `.md`.
    pub name: String,
    /// A sentence saying what the entry holds.
    pub job: String,
}

impl Extra {
    /// Whether the extension is a directory.
    #[must_use]
    pub fn directory(&self) -> bool {
        self.name.strip_suffix(".md").is_none()
    }
}

impl Settings {
    /// The options a call received, checked: a configuration error names the key that is wrong.
    ///
    /// # Errors
    /// No kind or an unknown one, or an extension that breaks the rules of an extension.
    pub fn read(options: Result<Self>) -> Result<Self> {
        let settings = options.map_err(|error| {
            Error::config(format!(
                "the [plugins.docs] options need kind, one of {}: {error}",
                standard::kind_names()
            ))
        })?;
        settings.check()?;
        settings.check_delegated()?;
        Ok(settings)
    }

    fn check(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        for (position, extra) in self.extra.iter().enumerate() {
            let key = format!("the [plugins.docs] options extra[{position}]");
            let name = extra.name.as_str();
            if !kebab(name) {
                return Err(Error::config(format!(
                    "{key}.name {name:?} is not kebab-case: write a directory as `playbooks`, or a Markdown file as `playbooks.md`"
                )));
            }
            let stem = name.strip_suffix(".md").unwrap_or(name);
            if let Some(entry) = standard::clash(name) {
                return Err(Error::config(format!(
                    "{key}.name {name:?} is the standard entry {}; an extension adds an entry the standard does not have",
                    entry.path()
                )));
            }
            if let Some(reserved) = standard::reserved(name) {
                return Err(Error::config(format!(
                    "{key}.name {name:?} is reserved: its content goes to {}",
                    reserved.goes_to
                )));
            }
            if extra.job.trim().is_empty() {
                return Err(Error::config(format!(
                    "{key}.job is empty: say in a sentence what {name} holds"
                )));
            }
            if !seen.insert(stem) {
                return Err(Error::config(format!(
                    "{key}.name {name:?} is declared twice; declare each extension once"
                )));
            }
        }
        Ok(())
    }

    fn check_delegated(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        for (position, name) in self.delegated.iter().enumerate() {
            let key = format!("the [plugins.docs] options delegated[{position}]");
            let Some(entry) = standard::entry(name).filter(|entry| entry.delegated.is_some())
            else {
                let choices: Vec<&str> = ENTRIES
                    .iter()
                    .filter(|entry| entry.delegated.is_some())
                    .map(|entry| entry.name)
                    .collect();
                return Err(Error::config(format!(
                    "{key} {name:?} is not a delegated entry; list one of {}",
                    choices.join(", ")
                )));
            };
            if let Cell::Absent(reason) = entry.cell(self.kind) {
                return Err(Error::config(format!(
                    "{key} {name:?} is not an entry of {}: {reason}",
                    self.kind.noun()
                )));
            }
            if !seen.insert(name.as_str()) {
                return Err(Error::config(format!(
                    "{key} {name:?} is listed twice; list each delegated entry once"
                )));
            }
        }
        Ok(())
    }

    /// Whether `name` is a delegated entry the options list.
    #[must_use]
    pub fn delegates(&self, name: &str) -> bool {
        self.delegated.iter().any(|area| area == name)
    }

    /// The declared extension named `name` (a top-level name under `docs/`).
    #[must_use]
    pub fn extra(&self, name: &str) -> Option<&Extra> {
        self.extra.iter().find(|extra| extra.name == name)
    }
}

/// Lowercase words of letters and digits joined by single hyphens, with an optional `.md`.
fn kebab(name: &str) -> bool {
    let stem = name.strip_suffix(".md").unwrap_or(name);
    !stem.is_empty()
        && stem.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

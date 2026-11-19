//! The baseline: `gate/baseline.json`, the fingerprints of findings accepted as they are today.
//!
//! A finding whose fingerprint is in the baseline is not reported; an entry that matches no finding
//! is, so a baseline only shrinks. `gate baseline write` rewrites the file from a full run.

use serde::{Deserialize, Serialize};

use crate::config::CORE_PLUGIN;
use crate::error::{Error, Result};
use crate::findings::{Finding, fingerprint};
use crate::snapshot::Snapshot;

/// The baseline's path, relative to the repository root.
pub const BASELINE_FILE: &str = "gate/baseline.json";

/// One accepted finding.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct BaselineEntry {
    /// The finding's path, relative to the repository root; `None` for a finding without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The project the finding was reported for; `None` for the repository scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The rule id.
    pub rule: String,
    /// The aspect; `""` when the rule has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub aspect: String,
    /// The message.
    pub message: String,
    /// The fingerprint of rule, aspect, path and message.
    pub fingerprint: String,
}

impl BaselineEntry {
    /// The entry of a finding.
    #[must_use]
    pub fn of(finding: &Finding) -> Self {
        Self {
            path: finding
                .location
                .as_ref()
                .map(|location| location.key().to_owned()),
            project: finding.project.clone(),
            rule: finding.rule.clone(),
            aspect: finding.aspect.clone(),
            message: finding.message.clone(),
            fingerprint: finding.fingerprint(),
        }
    }

    /// The rule with its aspect: `<plugin>/<rule>#<aspect>`, or the rule alone.
    #[must_use]
    pub fn rule_with_aspect(&self) -> String {
        if self.aspect.is_empty() {
            self.rule.clone()
        } else {
            format!("{}#{}", self.rule, self.aspect)
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BaselineFile {
    findings: Vec<BaselineEntry>,
}

/// The accepted findings, sorted by path, rule, aspect and message.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Baseline {
    entries: Vec<BaselineEntry>,
}

impl Baseline {
    /// Reads `gate/baseline.json` from the snapshot; empty when it is not tracked.
    ///
    /// # Errors
    /// Fails when the file is not the baseline format, or an entry's fingerprint does not match its
    /// fields (the file was edited by hand).
    pub fn load(snapshot: &Snapshot) -> Result<Self> {
        let Some(text) = snapshot.text(BASELINE_FILE)? else {
            return Ok(Self::default());
        };
        let file: BaselineFile = serde_json::from_str(&text).map_err(|error| {
            Error::config(format!(
                "{BASELINE_FILE}:{}:{}: {error}; rewrite it with gate baseline write",
                error.line(),
                error.column()
            ))
        })?;

        for entry in &file.findings {
            let expected = fingerprint(
                &entry.rule,
                &entry.aspect,
                entry.path.as_deref().unwrap_or(""),
                &entry.message,
            );
            if entry.fingerprint != expected {
                return Err(Error::config(format!(
                    "{BASELINE_FILE}: the entry for {} at {} has fingerprint {}, but its fields give {expected}; the file was edited by hand, rewrite it with gate baseline write",
                    entry.rule_with_aspect(),
                    entry.path.as_deref().unwrap_or("(no path)"),
                    entry.fingerprint
                )));
            }
        }

        Ok(Self::from_entries(file.findings))
    }

    /// The baseline of `findings`. The core's own findings (`gate/...`) are never accepted.
    #[must_use]
    pub fn from_findings(findings: &[Finding]) -> Self {
        Self::from_entries(
            findings
                .iter()
                .filter(|finding| !is_core(&finding.rule))
                .map(BaselineEntry::of)
                .collect(),
        )
    }

    fn from_entries(mut entries: Vec<BaselineEntry>) -> Self {
        entries.sort();
        Self { entries }
    }

    /// The entries.
    #[must_use]
    pub fn entries(&self) -> &[BaselineEntry] {
        &self.entries
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it accepts nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The file's contents: pretty JSON with a final newline.
    ///
    /// # Errors
    /// Fails when the entries cannot be serialized; nothing is written then.
    pub fn to_json(&self) -> Result<String> {
        let file = BaselineFile {
            findings: self.entries.clone(),
        };
        let mut text = serde_json::to_string_pretty(&file)?;
        text.push('\n');
        Ok(text)
    }
}

/// Whether a rule id is one of the core's own pseudo-rules.
#[must_use]
pub fn is_core(rule: &str) -> bool {
    rule.split_once('/')
        .is_some_and(|(plugin, _)| plugin == CORE_PLUGIN)
}

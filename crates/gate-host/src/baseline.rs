//! The baseline: `gate/baseline.json`, the fingerprints of findings accepted as they are today.
//!
//! A finding whose fingerprint is in the baseline is not reported; an entry that matches no finding
//! is, so a baseline only shrinks. `gate baseline write` writes the file from a full run, for the
//! first baseline; `gate baseline prune` drops the entries that match no finding and adds none.

use serde::{Deserialize, Serialize};

use crate::config::CORE_PLUGIN;
use crate::error::{Error, Result};
use crate::findings::{Finding, Identity, Location, RulesRan, fingerprint};
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
    /// The message, as the finding read when the entry was written.
    pub message: String,
    /// The plugin's stable key for the finding, when it gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The fingerprint of rule, aspect, path and key, or of rule, aspect, path and message when
    /// there is no key.
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
            key: finding.key.clone(),
            fingerprint: finding.fingerprint(),
        }
    }

    /// What the fingerprint digests besides rule, aspect and path: the key, else the message.
    #[must_use]
    pub fn identity(&self) -> Identity<'_> {
        match &self.key {
            Some(key) => Identity::Key(key),
            None => Identity::Message(&self.message),
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
        match snapshot.text(BASELINE_FILE)? {
            Some(text) => Self::parse(&text),
            None => Ok(Self::default()),
        }
    }

    /// The baseline in `text`, the contents of `gate/baseline.json`.
    ///
    /// # Errors
    /// As [`Baseline::load`].
    pub fn parse(text: &str) -> Result<Self> {
        let file: BaselineFile = serde_json::from_str(text).map_err(|error| {
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
                entry.identity(),
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

    /// Which of `findings` the baseline accepts, and which of its entries accept one: each entry
    /// accepts one finding of its fingerprint in its own project, the first in `findings`' order
    /// that no other entry took. The core's own findings (`gate/...`) are never accepted.
    #[must_use]
    pub fn accept(&self, findings: &[Finding]) -> Accepted {
        let mut entries = vec![false; self.entries.len()];
        let findings = findings
            .iter()
            .map(|finding| {
                if is_core(&finding.rule) {
                    return false;
                }
                let print = finding.fingerprint();
                let entry = self.entries.iter().enumerate().position(|(index, entry)| {
                    !entries[index]
                        && entry.fingerprint == print
                        && entry.project == finding.project
                });
                match entry {
                    Some(index) => {
                        entries[index] = true;
                        true
                    }
                    None => false,
                }
            })
            .collect();
        Accepted { findings, entries }
    }

    /// The entries `check` reports as stale: those that accept none of the findings (`accepted`,
    /// from [`Baseline::accept`]) although their rule ran in their scope (`ran`). An entry of a rule
    /// that did not run is not stale, since the run could not see its finding.
    #[must_use]
    pub fn stale(&self, accepted: &Accepted, ran: &RulesRan) -> Vec<&BaselineEntry> {
        self.entries
            .iter()
            .zip(&accepted.entries)
            .filter(|(entry, accepted)| is_stale(entry, **accepted, ran))
            .map(|(entry, _)| entry)
            .collect()
    }

    /// The baseline pruned against `findings`, a full run's findings before the baseline. An entry
    /// without a key that matches no finding, but whose fingerprint is the message fingerprint of a
    /// finding that now has a key (rule, aspect, path, project and message as the entry has them),
    /// is re-keyed: rewritten as that finding's entry. Then every stale entry
    /// ([`Baseline::stale`]) is removed. It never adds an entry: a finding no entry accepts stays
    /// reported.
    #[must_use]
    pub fn pruned(&self, findings: &[Finding], ran: &RulesRan) -> Pruned {
        let accepted = self.accept(findings);
        let mut taken = accepted.findings.clone();
        let mut kept = Vec::new();
        let mut rekeyed = Vec::new();
        let mut removed = Vec::new();
        for (entry, accepted) in self.entries.iter().zip(&accepted.entries) {
            if *accepted {
                kept.push(entry.clone());
                continue;
            }
            let now_keyed = if entry.key.is_none() {
                findings.iter().enumerate().position(|(index, finding)| {
                    !taken[index] && finding.key.is_some() && was_keyless(entry, finding)
                })
            } else {
                None
            };
            if let Some(index) = now_keyed {
                taken[index] = true;
                let entry = BaselineEntry::of(&findings[index]);
                kept.push(entry.clone());
                rekeyed.push(entry);
            } else if is_stale(entry, false, ran) {
                removed.push(entry.clone());
            } else {
                kept.push(entry.clone());
            }
        }
        Pruned {
            baseline: Self::from_entries(kept),
            rekeyed,
            removed,
        }
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

/// What [`Baseline::pruned`] did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Pruned {
    /// The baseline after pruning.
    pub baseline: Baseline,
    /// The entries re-keyed, as they are now: each was written without a key, for a finding that
    /// now has one.
    pub rekeyed: Vec<BaselineEntry>,
    /// The entries removed: stale ones.
    pub removed: Vec<BaselineEntry>,
}

/// Whether `entry`, written without a key, is `finding` as it read before its plugin keyed it: the
/// same rule, aspect, path and project, and the entry's fingerprint is the finding's message
/// fingerprint.
fn was_keyless(entry: &BaselineEntry, finding: &Finding) -> bool {
    let path = finding.location.as_ref().map_or("", Location::key);
    entry.project == finding.project
        && entry.rule == finding.rule
        && entry.aspect == finding.aspect
        && entry.path.as_deref().unwrap_or("") == path
        && entry.fingerprint
            == fingerprint(
                &finding.rule,
                &finding.aspect,
                path,
                Identity::Message(&finding.message),
            )
}

/// What [`Baseline::accept`] answers: per finding, whether an entry accepts it; per entry, whether
/// it accepts a finding.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Accepted {
    /// One flag per finding, in the order given.
    pub findings: Vec<bool>,
    /// One flag per entry, in the baseline's order.
    pub entries: Vec<bool>,
}

/// Whether `entry`, which accepted a finding or not, is stale: it accepted none although its rule ran
/// in its scope.
fn is_stale(entry: &BaselineEntry, accepted: bool, ran: &RulesRan) -> bool {
    !accepted && ran.ran(entry.project.as_deref(), &entry.rule)
}

/// Whether a rule id is one of the core's own pseudo-rules.
#[must_use]
pub fn is_core(rule: &str) -> bool {
    rule.split_once('/')
        .is_some_and(|(plugin, _)| plugin == CORE_PLUGIN)
}

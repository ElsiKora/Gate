//! The `[plugins.ledger] options` every ledger rule shares: where the ledger files are, the parity
//! plan, and the status vocabularies.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{CallState, Error, Run};

use crate::text::normal_path;

/// The statuses that close a record, compared with the status in lowercase.
pub const CLOSED_STATUSES: [&str; 7] = [
    "closed",
    "complete",
    "complete (external prerequisite)",
    "completed",
    "done",
    "passed",
    "verified",
];

/// The shared options of the ledger plugin, `[plugins.ledger] options`. Every path is relative to
/// the project root; a path outside the project needs a `reads` grant in `[plugins.ledger]`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Settings {
    /// The decision index: a table whose rows start with a backticked `D-` identifier.
    pub decisions: String,
    /// The entry file of the implementation records (`- `I-…`: title` items); the Markdown files
    /// of its same-stem directory belong to it.
    pub implementation: String,
    /// The entry file of the verification records (`- `V-…`: title` items).
    pub verification: String,
    /// The entry file of the follow-up records (`- `F-…`: …` items).
    pub follow_ups: String,
    /// The entry file of the production blocker records (`- `PB-…`: title` items).
    pub blockers: String,
    /// A plan whose headings pair with implementation and verification records; absent, the plan
    /// checks are off.
    pub parity: Option<Parity>,
    /// The statuses that close a record, compared with the status in lowercase.
    pub closed_statuses: Vec<String>,
    /// A status that starts with one of these is planned work.
    pub planned_prefixes: Vec<String>,
    /// A status that starts with one of these is started work.
    pub started_prefixes: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            decisions: "docs/ledger/decision-index.md".to_owned(),
            implementation: "docs/ledger/implementation-slices.md".to_owned(),
            verification: "docs/ledger/verification.md".to_owned(),
            follow_ups: "docs/ledger/follow-ups.md".to_owned(),
            blockers: "docs/ledger/production-blockers.md".to_owned(),
            parity: None,
            closed_statuses: CLOSED_STATUSES.map(str::to_owned).to_vec(),
            planned_prefixes: vec!["planned".to_owned()],
            started_prefixes: vec!["in progress".to_owned(), "in-progress".to_owned()],
        }
    }
}

/// A plan whose `### `I-<prefix>-…`: title` headings each pair with an implementation record of the
/// same identifier and a verification record `V-<prefix>-…`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Parity {
    /// The entry file of the plan; the Markdown files of its same-stem directory belong to it.
    pub plan: String,
    /// The identifier part after `I-` and `V-` that marks a paired record: uppercase letters and
    /// digits.
    pub prefix: String,
    /// The name of the paired work in messages.
    pub label: String,
    /// The plan field that holds the full commit id of the reference the work follows.
    pub reference: String,
    /// The product names a closing note or a closer may put before a record identifier.
    #[serde(default)]
    pub products: Vec<String>,
    /// The other plan fields an active plan record must carry, each exactly once.
    pub fields: Vec<String>,
    /// The plan status that makes the reference and the fields required.
    pub active_status: String,
}

/// Checks that the option `name` holds a relative path in normal form ([`normal_path`]): the tree
/// names each file one way only, so another spelling would match nothing.
///
/// # Errors
/// When the path is absolute or not in normal form; the message shows the normal form.
pub fn check_path(name: &str, path: &str) -> gate_sdk::Result<()> {
    match normal_path(path) {
        Some(normal) if normal == path => Ok(()),
        Some(normal) => Err(Error::config(format!(
            "{name} {path:?} is not in normal form; write {normal:?}"
        ))),
        None => Err(Error::config(format!(
            "{name} {path:?} is absolute; write it relative to the project root"
        ))),
    }
}

impl Settings {
    /// Reads and checks the shared options of a run.
    ///
    /// # Errors
    /// When they do not match their type, a path is not in normal form, a closed status is not
    /// lowercase, or the parity prefix is not uppercase letters and digits.
    pub fn read<O, S: CallState>(run: &Run<'_, O, S>) -> gate_sdk::Result<Self> {
        Self::checked(run.plugin_options()?)
    }

    /// Checks shared options a generator read with [`gate_sdk::Generate::plugin_options`].
    ///
    /// # Errors
    /// As [`Settings::read`].
    pub fn checked(settings: Self) -> gate_sdk::Result<Self> {
        let mut paths = vec![
            ("decisions", &settings.decisions),
            ("implementation", &settings.implementation),
            ("verification", &settings.verification),
            ("follow-ups", &settings.follow_ups),
            ("blockers", &settings.blockers),
        ];
        if let Some(parity) = &settings.parity {
            paths.push(("parity.plan", &parity.plan));
        }
        for (name, path) in paths {
            check_path(&format!("[plugins.ledger] options {name}"), path)?;
        }
        if let Some(status) = settings
            .closed_statuses
            .iter()
            .find(|status| status.to_lowercase() != **status)
        {
            return Err(Error::config(format!(
                "[plugins.ledger] options closed-statuses has {status:?}; statuses are compared in lowercase, so write {:?}",
                status.to_lowercase()
            )));
        }

        if let Some(parity) = &settings.parity
            && (parity.prefix.is_empty()
                || !parity
                    .prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()))
        {
            return Err(Error::config(format!(
                "[plugins.ledger] options parity.prefix {:?} must be uppercase letters and digits, such as \"CORE\"",
                parity.prefix
            )));
        }

        Ok(settings)
    }

    /// Whether a status is closed: in lowercase, one of the closed statuses.
    #[must_use]
    pub fn is_closed(&self, status: &str) -> bool {
        let lower = status.to_lowercase();
        self.closed_statuses.contains(&lower)
    }

    /// Whether a status is closed, or its first word is a closed status, such as
    /// `completed and archived`.
    #[must_use]
    pub fn is_settled(&self, status: &str) -> bool {
        self.is_closed(status)
            || status
                .split(|character: char| character.is_whitespace() || ";,(.:".contains(character))
                .next()
                .is_some_and(|word| self.is_closed(word))
    }

    /// Whether a status is planned work that has not started.
    #[must_use]
    pub fn is_planned(&self, status: &str) -> bool {
        self.planned_prefixes
            .iter()
            .any(|prefix| status.starts_with(prefix.as_str()))
    }

    /// Whether a status is started work.
    #[must_use]
    pub fn is_started(&self, status: &str) -> bool {
        self.started_prefixes
            .iter()
            .any(|prefix| status.starts_with(prefix.as_str()))
    }

    /// Whether a status is open: planned or started.
    #[must_use]
    pub fn is_open(&self, status: &str) -> bool {
        self.is_planned(status) || self.is_started(status)
    }
}

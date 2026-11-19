//! `ledger/records` and `ledger/record-status`: ledger records keep their fields, counterparts,
//! commits, dependencies and closure evidence, and paired records agree on an allowed status.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Rule, Run};

use crate::catalog::Catalog;
use crate::records::{PENDING_PATTERNS, PENDING_VALUES, Tracking, analyse};
use crate::settings::Settings;
use crate::text::pattern;

/// The aspect `ledger/record-status` reports; `ledger/records` leaves it out.
const STATUS: &str = "record-status";

/// The rule `ledger/records`.
#[derive(Debug, Default)]
pub struct Records;

/// The options of `ledger/records`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct RecordsOptions {
    /// Values that are placeholders rather than evidence, compared after the value is normalized:
    /// links reduced to their text, markup and trailing `.;:` removed, white space collapsed,
    /// lowercase.
    pub pending_values: Vec<String>,
    /// Patterns that make a normalized value a placeholder; `\d`, `\w` and `\b` are ASCII, and `\s`
    /// is the white space set of the rule pages.
    pub pending_patterns: Vec<String>,
    /// The phases a production blocker may have; empty allows any.
    pub blocker_phases: Vec<String>,
    /// An unfinished paired record (its plan status is not a closed status, compared as written)
    /// keeps `Commit` `pending` in both the plan and the implementation record.
    pub unfinished_commit_pending: bool,
}

impl Default for RecordsOptions {
    fn default() -> Self {
        Self {
            pending_values: PENDING_VALUES.map(str::to_owned).to_vec(),
            pending_patterns: PENDING_PATTERNS.map(str::to_owned).to_vec(),
            blocker_phases: Vec::new(),
            unfinished_commit_pending: false,
        }
    }
}

impl Rule for Records {
    type Options = RecordsOptions;

    const NAME: &'static str = "records";
    const TITLE: &'static str = "Ledger records keep their fields, counterparts, commits, dependencies and closure evidence";
    const ASPECTS: &'static [&'static str] = &[
        "dependency-cycle",
        "parity-fields",
        "record-closure",
        "record-commit",
        "record-counterpart",
        "record-dependency",
        "record-field",
        "record-shape",
        "record-unique",
    ];

    fn check(&self, run: &mut Run<'_, RecordsOptions>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let options = run.options();
        let tree = run.tree();
        let pending_patterns = options
            .pending_patterns
            .iter()
            .map(|source| {
                pattern(source).map_err(|error| {
                    Error::new(format!(
                        "ledger/records option pending-patterns has the invalid pattern {source:?}: {error}; fix or remove it"
                    ))
                })
            })
            .collect::<gate_sdk::Result<Vec<_>>>()?;

        if !tree.contains(&settings.implementation) {
            return Ok(());
        }

        let tracking = Tracking {
            settings: &settings,
            pending_values: &options.pending_values,
            pending_patterns,
            blocker_phases: &options.blocker_phases,
            allowed: None,
            unfinished_commit_pending: options.unfinished_commit_pending,
        };
        let findings = analyse(&Catalog::new(tree), &tracking)?;
        findings.write(run.report(), |aspect| aspect != STATUS, |aspect| aspect);
        Ok(())
    }
}

/// The rule `ledger/record-status`.
#[derive(Debug, Default)]
pub struct RecordStatus;

/// The options of `ledger/record-status`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct RecordStatusOptions {
    /// The statuses a plan, implementation or verification record of a paired plan record may
    /// have, compared as written; absent allows any.
    #[schemars(length(min = 1))]
    pub allowed: Option<Vec<String>>,
}

impl Rule for RecordStatus {
    type Options = RecordStatusOptions;

    const NAME: &'static str = "record-status";
    const TITLE: &'static str =
        "A paired plan record, its implementation and its verification share one allowed status";

    fn check(&self, run: &mut Run<'_, RecordStatusOptions>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let options = run.options();
        let tree = run.tree();

        if options.allowed.as_ref().is_some_and(Vec::is_empty) {
            return Err(Error::new(
                "ledger/record-status option allowed is empty; list the allowed statuses or remove the option",
            ));
        }

        if !tree.contains(&settings.implementation) {
            return Ok(());
        }

        let tracking = Tracking {
            settings: &settings,
            pending_values: &[],
            pending_patterns: Vec::new(),
            blocker_phases: &[],
            allowed: options.allowed.as_deref(),
            unfinished_commit_pending: false,
        };
        let findings = analyse(&Catalog::new(tree), &tracking)?;
        findings.write(run.report(), |aspect| aspect == STATUS, |_| "");
        Ok(())
    }
}

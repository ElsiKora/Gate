//! `ledger/records` and `ledger/record-status`: ledger records keep their fields, counterparts,
//! commits, dependencies and closure evidence, and paired records agree on an allowed status.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Need, Rule, Run};

use crate::catalog::Catalog;
use crate::input::{self, INPUT};
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

impl Rule<crate::LedgerCall> for Records {
    type Options = RecordsOptions;

    const NAME: &'static str = "records";
    const TITLE: &'static str = "Ledger records keep their fields, counterparts, commits, dependencies and closure evidence";
    const ASPECTS: &'static [&'static str] = &[
        "dependency-cycle",
        INPUT,
        "parity-fields",
        "record-closure",
        "record-commit",
        "record-counterpart",
        "record-dependency",
        "record-field",
        "record-shape",
        "record-unique",
    ];

    fn check(&self, run: &mut Run<'_, RecordsOptions, crate::LedgerCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let pending_patterns = run
            .options()
            .pending_patterns
            .iter()
            .map(|source| {
                pattern(source).map_err(|error| {
                    Error::config(format!(
                        "ledger/records option pending-patterns has the invalid pattern {source:?}: {error}; fix or remove it"
                    ))
                })
            })
            .collect::<gate_sdk::Result<Vec<_>>>()?;

        if !input::tracked(run.report(), &tree, Self::NAME, &input::records(&settings)) {
            return Ok(());
        }

        let options = run.options();
        let tracking = Tracking {
            settings: &settings,
            pending_values: &options.pending_values,
            pending_patterns,
            blocker_phases: &options.blocker_phases,
            allowed: None,
            unfinished_commit_pending: options.unfinished_commit_pending,
        };
        let findings = analyse(shared.catalog(), &tracking)?;
        findings.write(run.report(), |aspect| aspect != STATUS, |aspect| aspect);
        Ok(())
        })
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

impl Rule<crate::LedgerCall> for RecordStatus {
    type Options = RecordStatusOptions;

    const NAME: &'static str = "record-status";
    const TITLE: &'static str =
        "A paired plan record, its implementation and its verification share one allowed status";
    const ASPECTS: &'static [&'static str] = &[INPUT];
    const NEEDS: &'static [Need] = &[Need {
        key: "parity",
        when: None,
        why: "the rule compares the statuses of the paired plan, implementation and verification records: set parity to the plan whose headings pair with them",
    }];

    fn check(
        &self,
        run: &mut Run<'_, RecordStatusOptions, crate::LedgerCall>,
    ) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
        let settings = Settings::read(run)?;
        let tree = run.tree();

        if run.options().allowed.as_ref().is_some_and(Vec::is_empty) {
            return Err(Error::config(
                "ledger/record-status option allowed is empty; list the allowed statuses or remove the option",
            ));
        }

        if !input::tracked(run.report(), &tree, Self::NAME, &input::records(&settings)) {
            return Ok(());
        }

        let options = run.options();
        let tracking = Tracking {
            settings: &settings,
            pending_values: &[],
            pending_patterns: Vec::new(),
            blocker_phases: &[],
            allowed: options.allowed.as_deref(),
            unfinished_commit_pending: false,
        };
        let findings = analyse(shared.catalog(), &tracking)?;
        findings.write(run.report(), |aspect| aspect == STATUS, |_| "");
        Ok(())
        })
    }
}

//! `ledger/record-status`.

use crate::{IMPLEMENTATION, PARITY, PLAN, VERIFICATION, error, findings, input, ledger, seen};

#[test]
fn matching_statuses_pass() {
    assert_eq!(findings("record-status", "", PARITY, &ledger()), []);
    assert_eq!(
        findings(
            "record-status",
            r#"{"allowed":["completed","planned"]}"#,
            PARITY,
            &ledger()
        ),
        []
    );
}

#[test]
fn paired_records_with_different_statuses() {
    let files = ledger().edit(VERIFICATION, "`completed`", "`passed`");
    assert_eq!(
        findings("record-status", "", PARITY, &files),
        [seen(
            "",
            PLAN,
            Some(3),
            "I-ALPHA-ONE-001 and V-ALPHA-ONE-001 must have exactly matching Status values"
        )]
    );
}

#[test]
fn a_status_outside_the_allowed_list_is_reported_once_per_file() {
    assert_eq!(
        findings(
            "record-status",
            r#"{"allowed":["planned","Completed"]}"#,
            PARITY,
            &ledger()
        ),
        [
            seen(
                "",
                PLAN,
                Some(3),
                "I-ALPHA-ONE-001 has unsupported Status completed in docs/ledger/plans/alpha.md"
            ),
            seen(
                "",
                PLAN,
                Some(3),
                "I-ALPHA-ONE-001 has unsupported Status completed in docs/ledger/implementation-slices.md"
            ),
            seen(
                "",
                PLAN,
                Some(3),
                "I-ALPHA-ONE-001 has unsupported Status completed in docs/ledger/verification.md"
            ),
        ]
    );
}

#[test]
fn a_missing_implementation_file_or_plan_is_an_input_finding() {
    // Without parity the rule pairs nothing; the host refuses that configuration, since the rule
    // needs parity (`x-gate-needs`), and the end-to-end tests run that refusal.
    let files = ledger().edit(VERIFICATION, "`completed`", "`passed`");
    assert_eq!(findings("record-status", "", "", &files), []);
    assert_eq!(
        findings("record-status", "", PARITY, &files.without(IMPLEMENTATION)),
        [input(
            "record-status",
            IMPLEMENTATION,
            "[plugins.ledger] options implementation"
        )]
    );
    assert_eq!(
        findings("record-status", "", PARITY, &ledger().without(PLAN)),
        [input(
            "record-status",
            PLAN,
            "[plugins.ledger] options parity.plan"
        )]
    );
}

#[test]
fn an_empty_allowed_list_is_an_error() {
    let message = error("record-status", r#"{"allowed":[]}"#, PARITY, &ledger());
    assert!(message.contains("allowed is empty"), "{message}");
}

//! `ledger/records`.

use crate::{
    BLOCKERS, Files, IMPLEMENTATION, OID_A, OID_B, PARITY, PLAN, VERIFICATION, error, findings,
    input, ledger, seen,
};

fn records(files: &Files) -> Vec<crate::Seen> {
    findings("records", "", PARITY, files)
}

#[test]
fn a_consistent_ledger_passes() {
    assert_eq!(records(&ledger()), []);
}

#[test]
fn an_untracked_implementation_file_or_parity_plan_is_an_input_finding() {
    let files = ledger().without(IMPLEMENTATION).without(BLOCKERS);
    assert_eq!(
        records(&files),
        [input(
            "records",
            IMPLEMENTATION,
            "[plugins.ledger] options implementation"
        )]
    );
    assert_eq!(
        records(&ledger().without(PLAN)),
        [input(
            "records",
            PLAN,
            "[plugins.ledger] options parity.plan"
        )]
    );
}

#[test]
fn record_shape_a_missing_blocker_file() {
    assert_eq!(
        records(&ledger().without(BLOCKERS)),
        [seen(
            "record-shape",
            BLOCKERS,
            None,
            "docs/ledger/production-blockers.md must contain at least one production blocker record"
        )]
    );
}

#[test]
fn record_shape_an_untitled_and_a_malformed_record() {
    let files = ledger().edit(
        IMPLEMENTATION,
        "- `I-BASE-002`: Base work",
        "- `I-BASE-002`: ",
    );
    assert_eq!(
        records(&files),
        [seen(
            "record-shape",
            IMPLEMENTATION,
            Some(9),
            "docs/ledger/implementation-slices.md I-BASE-002 must have a non-empty title"
        )]
    );

    let files = ledger().edit(
        IMPLEMENTATION,
        "- `I-BASE-002`: Base work",
        "- I-BASE-002 Base work",
    );
    let found = records(&files);
    assert!(
        found.contains(&seen(
            "record-shape",
            IMPLEMENTATION,
            Some(9),
            "docs/ledger/implementation-slices.md contains malformed implementation record definitions"
        )),
        "{found:#?}"
    );
}

#[test]
fn record_unique_a_repeated_identifier() {
    let files = ledger().edit(
        IMPLEMENTATION,
        "- `I-BASE-002`: Base work",
        "- `I-BASE-002`: Base work\n- `I-BASE-002`: Again",
    );
    let found = records(&files);
    assert!(
        found.contains(&seen(
            "record-unique",
            IMPLEMENTATION,
            Some(10),
            "docs/ledger/implementation-slices.md contains duplicate implementation record IDs"
        )),
        "{found:#?}"
    );
}

#[test]
fn record_field_a_status_that_is_not_backticked() {
    let files = ledger().edit(PLAN, "- **Status:** `completed`", "- **Status:** completed");
    let found = records(&files);
    assert!(
        found.contains(&seen(
            "record-field",
            PLAN,
            Some(3),
            "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 Status must be one non-empty backticked value"
        )),
        "{found:#?}"
    );
}

#[test]
fn record_field_a_missing_owner_and_a_repeated_optional_field() {
    let files = ledger().edit(PLAN, "- **Owning repository:** app\n", "");
    assert_eq!(
        records(&files),
        [seen(
            "record-field",
            PLAN,
            Some(3),
            "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 must contain exactly one non-empty owning repository field"
        )]
    );

    let files = ledger().edit(
        IMPLEMENTATION,
        "  - Blocked by: none\n  - Reuses",
        "  - Blocked by: none\n  - Blocked by: none\n  - Reuses",
    );
    assert_eq!(
        records(&files),
        [seen(
            "record-field",
            IMPLEMENTATION,
            Some(9),
            "docs/ledger/implementation-slices.md I-BASE-002 must not contain duplicate or empty Blocked by fields"
        )]
    );
}

#[test]
fn record_counterpart_a_missing_verification_record() {
    let files = ledger().edit(
        VERIFICATION,
        "- `V-ALPHA-ONE-001`: First slice checks",
        "- `V-ALPHA-TWO-001`: Other checks",
    );
    let found = records(&files);
    assert!(
        found.contains(&seen(
            "record-counterpart",
            VERIFICATION,
            None,
            "docs/ledger/verification.md is missing V-ALPHA-ONE-001 for I-ALPHA-ONE-001"
        )),
        "{found:#?}"
    );
    assert!(
        found.contains(&seen(
            "record-counterpart",
            VERIFICATION,
            Some(3),
            "docs/ledger/verification.md contains V-ALPHA-TWO-001 without matching I-ALPHA-TWO-001"
        )),
        "{found:#?}"
    );
}

#[test]
fn record_commit_plan_and_implementation_differ() {
    let files = ledger().edit(
        IMPLEMENTATION,
        &format!("  - Commit: `{OID_A}`"),
        &format!("  - Commit: `{OID_B}`"),
    );
    assert_eq!(
        records(&files),
        [seen(
            "record-commit",
            PLAN,
            Some(3),
            &format!("I-ALPHA-ONE-001 commits differ: plan={OID_A}, implementation={OID_B}")
        )]
    );
}

#[test]
fn record_commit_a_closed_record_without_a_full_commit() {
    let files = ledger().edit(PLAN, &format!("`{OID_A}`"), "`abc123`").edit(
        IMPLEMENTATION,
        &format!("`{OID_A}`"),
        "`abc123`",
    );
    assert_eq!(
        records(&files),
        [
            seen(
                "record-commit",
                PLAN,
                Some(3),
                "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 is closed without a backticked 40-hex Commit OID"
            ),
            seen(
                "record-commit",
                IMPLEMENTATION,
                Some(3),
                "docs/ledger/implementation-slices.md I-ALPHA-ONE-001 is closed without a backticked 40-hex Commit OID"
            ),
        ]
    );
}

#[test]
fn record_commit_an_unfinished_record_keeps_its_commit_pending_only_when_asked() {
    let files = ledger()
        .edit(PLAN, "`completed`", "`planned`")
        .edit(
            IMPLEMENTATION,
            "  - Status: `completed`",
            "  - Status: `planned`",
        )
        .edit(VERIFICATION, "`completed`", "`planned`");
    assert_eq!(records(&files), []);
    assert_eq!(
        findings(
            "records",
            r#"{"unfinished-commit-pending":true}"#,
            PARITY,
            &files
        ),
        [seen(
            "record-commit",
            PLAN,
            Some(3),
            "I-ALPHA-ONE-001 is unfinished and must keep its commit pending"
        )]
    );
}

#[test]
fn record_dependency_an_unknown_and_a_repeated_dependency() {
    let files = ledger().edit(
        IMPLEMENTATION,
        "  - Blocked by: none\n  - Reuses",
        "  - Blocked by: `I-NOPE-009`, `F-ONE`, `F-ONE`\n  - Reuses",
    );
    assert_eq!(
        records(&files),
        [
            seen(
                "record-dependency",
                IMPLEMENTATION,
                Some(9),
                "docs/ledger/implementation-slices.md I-BASE-002 references unknown dependency I-NOPE-009"
            ),
            seen(
                "record-dependency",
                IMPLEMENTATION,
                Some(9),
                "docs/ledger/implementation-slices.md I-BASE-002 contains duplicate dependency F-ONE"
            ),
        ]
    );
}

#[test]
fn record_dependency_a_paired_list_that_is_not_backticked() {
    let files = ledger().edit(
        PLAN,
        "- **Blocked by:** none",
        "- **Blocked by:** the base work",
    );
    let found = records(&files);
    assert!(
        found.contains(&seen(
            "record-dependency",
            PLAN,
            Some(3),
            "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 Blocked by must be none or a comma-separated list of backticked I-* or F-* IDs"
        )),
        "{found:#?}"
    );
}

#[test]
fn dependency_cycle_two_records_that_wait_for_each_other() {
    let files = ledger()
        .edit(
            PLAN,
            "- **Blocked by:** none",
            "- **Blocked by:** `I-BASE-002`",
        )
        .edit(
            IMPLEMENTATION,
            "  - Blocked by: none\n  - Verification",
            "  - Blocked by: `I-BASE-002`\n  - Verification",
        )
        .edit(
            IMPLEMENTATION,
            "  - Blocked by: none\n  - Reuses",
            "  - Blocked by: `I-ALPHA-ONE-001`\n  - Reuses",
        );
    let found = records(&files);
    assert!(
        found.contains(&seen(
            "dependency-cycle",
            IMPLEMENTATION,
            Some(3),
            "docs/ledger/implementation-slices.md contains a cycle reachable from a Alpha slice: I-ALPHA-ONE-001 -> I-BASE-002 -> I-ALPHA-ONE-001"
        )),
        "{found:#?}"
    );
}

#[test]
fn record_closure_closed_records_with_placeholder_evidence() {
    let files = ledger().edit(
        VERIFICATION,
        "the unit tests pass on the merged branch",
        "[TBD](#later).",
    );
    assert_eq!(
        records(&files),
        [seen(
            "record-closure",
            VERIFICATION,
            Some(3),
            "docs/ledger/verification.md V-ALPHA-ONE-001 is closed without concrete verification notes"
        )]
    );

    let files = ledger().edit(BLOCKERS, "`open`", "`closed`");
    assert_eq!(
        records(&files),
        [
            seen(
                "record-closure",
                BLOCKERS,
                Some(3),
                "docs/ledger/production-blockers.md PB-ONE is closed without non-placeholder Exit evidence"
            ),
            seen(
                "record-closure",
                BLOCKERS,
                Some(3),
                "docs/ledger/production-blockers.md PB-ONE is closed with unresolved prerequisite I-BASE-002"
            ),
        ]
    );
}

#[test]
fn pending_values_and_patterns_are_options() {
    let files = ledger().edit(
        VERIFICATION,
        "the unit tests pass on the merged branch",
        "awaiting review",
    );
    assert_eq!(records(&files), []);
    assert_eq!(
        findings(
            "records",
            r#"{"pending-patterns":["^awaiting\\b"]}"#,
            PARITY,
            &files
        )
        .len(),
        1
    );
    assert_eq!(
        findings(
            "records",
            r#"{"pending-values":["awaiting review"]}"#,
            PARITY,
            &files
        )
        .len(),
        1
    );
}

#[test]
fn parity_fields_an_active_record_needs_the_reference_and_the_fields() {
    let active = |files: Files| {
        files
            .edit(PLAN, "`completed`", "`in progress`")
            .edit(
                IMPLEMENTATION,
                "  - Status: `completed`",
                "  - Status: `in progress`",
            )
            .edit(VERIFICATION, "`completed`", "`in progress`")
    };
    assert_eq!(
        records(&active(ledger())),
        [
            seen(
                "parity-fields",
                PLAN,
                Some(3),
                "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 must contain exactly one non-empty Upstream reference field"
            ),
            seen(
                "parity-fields",
                PLAN,
                Some(3),
                "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 must contain exactly one non-empty Parity evidence field"
            ),
        ]
    );

    let files = active(ledger()).edit(
        PLAN,
        "- **Owning repository:** app\n",
        "- **Owning repository:** app\n- **Upstream reference:** `abc`\n- **Parity evidence:** the suites match\n",
    );
    assert_eq!(
        records(&files),
        [seen(
            "parity-fields",
            PLAN,
            Some(3),
            "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 must use a full Upstream reference OID"
        )]
    );
}

#[test]
fn blocker_phases_restrict_the_phase_only_when_listed() {
    assert_eq!(
        findings(
            "records",
            r#"{"blocker-phases":["SHIP"]}"#,
            PARITY,
            &ledger()
        ),
        [seen(
            "record-field",
            BLOCKERS,
            Some(3),
            "docs/ledger/production-blockers.md PB-ONE uses unsupported Phase LAUNCH"
        )]
    );
}

#[test]
fn without_parity_the_paired_checks_are_off() {
    let files = ledger().without(PLAN).without(VERIFICATION);
    assert_eq!(findings("records", "", "", &files), []);
}

#[test]
fn option_errors_name_the_option() {
    let message = error(
        "records",
        r#"{"pending-patterns":["("]}"#,
        PARITY,
        &ledger(),
    );
    assert!(message.contains("pending-patterns"), "{message}");

    let message = error("records", r#"{"pending":[]}"#, PARITY, &ledger());
    assert!(message.contains("unknown field `pending`"), "{message}");

    let message = error(
        "records",
        "",
        r#"{"parity":{"plan":"p.md","prefix":"al-pha","label":"A","reference":"R","fields":[],"active-status":"x"}}"#,
        &ledger(),
    );
    assert!(message.contains("parity.prefix"), "{message}");

    let message = error("records", "", r#"{"implementations":"x.md"}"#, &ledger());
    assert!(message.contains("[plugins] options"), "{message}");
}

//! `ledger/closing-notes`.

use crate::{Files, IMPLEMENTATION, OID_A, PARITY, PLAN, error, findings, input, ledger, seen};

const REUSES: &str = "  - Reuses: none; this work builds the first parser itself.\n";

/// The ledger with a temporary-difference field on the planned record, on line 13.
fn with_delta(field: &str) -> Files {
    ledger().edit(IMPLEMENTATION, REUSES, &format!("{REUSES}{field}\n"))
}

fn note(product: &str) -> String {
    format!("Closed (2026-01-02) by the merge of {product}`I-ALPHA-ONE-001` as `{OID_A}`.")
}

/// The options that name the field of the fixtures.
const FIELD: &str = r#"{"field":"`TEMPORARY_DELTA` records"}"#;

fn closing(files: &Files) -> Vec<crate::Seen> {
    findings("closing-notes", FIELD, PARITY, files)
}

const PREFIX: &str = "docs/ledger/implementation-slices.md I-BASE-002 has a TEMPORARY_DELTA entry";

#[test]
fn a_closed_closer_with_its_note_and_an_open_closer_without_one_pass() {
    assert_eq!(closing(&ledger()), []);
    let files = with_delta(&format!(
        "  - `TEMPORARY_DELTA` records: (1) the cache stays local; closer `I-ALPHA-ONE-001`. {}",
        note("Alpha ")
    ));
    assert_eq!(closing(&files), []);
    let files = with_delta(
        "  - `TEMPORARY_DELTA` records: (1) the cache stays local; closer `I-BASE-002`.",
    );
    assert_eq!(closing(&files), []);
}

#[test]
fn a_closed_closer_without_a_note() {
    let files = with_delta(
        "  - `TEMPORARY_DELTA` records: (1) the cache stays local; closed for Alpha by Alpha `I-ALPHA-ONE-001`.",
    );
    assert_eq!(
        closing(&files),
        [seen(
            "",
            IMPLEMENTATION,
            Some(13),
            &format!(
                "{PREFIX} whose exact closer I-ALPHA-ONE-001 is `completed`; add the note `Closed (YYYY-MM-DD) by the merge of `I-...` as `<full OID>`` naming the merge that closed it"
            )
        )]
    );
}

#[test]
fn product_names_come_from_parity() {
    let files = with_delta(&format!(
        "  - `TEMPORARY_DELTA` records: (1) the cache stays local; closer `I-ALPHA-ONE-001`. {}",
        note("Alpha ")
    ));
    assert_eq!(findings("closing-notes", FIELD, "", &files).len(), 1);
}

#[test]
fn the_field_is_required_and_a_missing_input_is_reported() {
    let files = with_delta("  - `TEMPORARY_DELTA` records: (1) closer `I-ALPHA-ONE-001`.");
    assert_eq!(closing(&files).len(), 1);
    let message = error("closing-notes", "", PARITY, &files);
    assert!(message.contains("missing field `field`"), "{message}");
    assert_eq!(
        error("closing-notes", r#"{"field":" "}"#, PARITY, &files),
        "rule closing-notes: ledger/closing-notes option field is empty; name the record field that lists the temporary differences"
    );
    assert_eq!(
        closing(&files.without(PLAN)),
        [input(
            "closing-notes",
            PLAN,
            "[plugins.ledger] options parity.plan"
        )]
    );
}

#[test]
fn two_notes_and_a_note_before_the_closer_closes() {
    let files = with_delta(&format!(
        "  - `TEMPORARY_DELTA` records: (1) closer `I-ALPHA-ONE-001`. {} {}",
        note(""),
        note("")
    ));
    assert_eq!(
        closing(&files),
        [seen(
            "",
            IMPLEMENTATION,
            Some(13),
            &format!(
                "{PREFIX} with 2 closing notes; keep the one note naming the merge that closed it, or split the entry at labels"
            )
        )]
    );

    let files = with_delta(&format!(
        "  - `TEMPORARY_DELTA` records: (1) closer `I-BASE-002`. {}",
        note("")
    ));
    assert_eq!(
        closing(&files),
        [seen(
            "",
            IMPLEMENTATION,
            Some(13),
            &format!(
                "{PREFIX} with a closing note while its exact closer I-BASE-002 is still `planned`; remove the note until the closer closes, or name the record that closed the difference as its closer"
            )
        )]
    );
}

#[test]
fn entries_split_at_labels_and_nested_items_keep_their_lines() {
    let field =
        "  - `TEMPORARY_DELTA` records: (1) closer `I-BASE-002`. (A2) closer `I-ALPHA-ONE-001`.";
    assert_eq!(closing(&with_delta(field)), []);
    assert_eq!(
        findings(
            "closing-notes",
            r#"{"field":"`TEMPORARY_DELTA` records","entry-label-letters":["A"]}"#,
            PARITY,
            &with_delta(field)
        )
        .len(),
        1
    );

    let files = with_delta(
        "  - `TEMPORARY_DELTA` records:\n    - (1) closer `I-BASE-002`.\n    - (2) closer\n      `I-ALPHA-ONE-001`.",
    );
    let found = closing(&files);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].2, Some(15));
}

#[test]
fn a_none_led_field_drops_its_leading_text() {
    let files = with_delta(
        "  - `TEMPORARY_DELTA` records: none remain since closer `I-ALPHA-ONE-001` merged",
    );
    assert_eq!(closing(&files), []);
    let files = with_delta(
        "  - `TEMPORARY_DELTA` records: some remain until closer `I-ALPHA-ONE-001` merged",
    );
    assert_eq!(closing(&files).len(), 1);
}

#[test]
fn the_field_is_an_option() {
    let files = with_delta("  - `DELTA` notes: (1) closer `I-ALPHA-ONE-001`.");
    assert_eq!(closing(&files), []);
    let found = findings(
        "closing-notes",
        r#"{"field":"`DELTA` notes"}"#,
        PARITY,
        &files,
    );
    assert_eq!(found.len(), 1);
    assert!(
        found[0].3.contains("has a DELTA entry whose exact closer"),
        "{found:#?}"
    );
}

#[test]
fn an_entry_label_letter_must_be_one_uppercase_letter() {
    let message = error(
        "closing-notes",
        r#"{"field":"`DELTA` notes","entry-label-letters":["ab"]}"#,
        PARITY,
        &ledger(),
    );
    assert!(message.contains("entry-label-letters"), "{message}");
}

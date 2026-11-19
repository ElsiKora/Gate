//! `ledger/reuses`.

use crate::{Files, IMPLEMENTATION, PARITY, error, findings, keys, ledger, seen};

const CATALOG: &str = "docs/reference/mechanisms.md";
const OPTIONS: &str = r#"{"catalog":"docs/reference/mechanisms.md"}"#;
const REUSES: &str = "  - Reuses: none; this work builds the first parser itself.";
const PREFIX: &str = "docs/ledger/implementation-slices.md I-BASE-002";

fn tree() -> Files {
    ledger()
        .with(
            CATALOG,
            "# Shared mechanisms\n\
             \n\
             - `shared-parser`: the parser\n\
             \x20 - Decisions: `D-ONE-001`\n\
             \x20 - Path: `src/parser.ts`, `src/parser/`\n\
             - `event-bus`: the bus\n\
             \x20 - Decisions: `D-ONE-001`\n\
             \x20 - Planned by: `I-BASE-002`\n",
        )
        .with("src/parser.ts", "export {};\n")
        .with("src/parser/read.ts", "export {};\n")
        .with(
            "docs/ledger/decision-index.md",
            "| `D-ONE-001` | `accepted` | One | `one.md` |\n| `D-TWO-001` | `proposed` | Two | `two.md` |\n",
        )
}

fn reuses(files: &Files) -> Vec<crate::Seen> {
    findings("reuses", OPTIONS, PARITY, files)
}

#[test]
fn a_specific_reason_and_known_keys_pass() {
    assert_eq!(reuses(&tree()), []);
    let files = tree().edit(IMPLEMENTATION, REUSES, "  - Reuses: `shared-parser`.");
    assert_eq!(reuses(&files), []);
}

#[test]
fn the_rule_is_off_without_a_catalog_or_an_implementation_file() {
    let files = tree().edit(IMPLEMENTATION, REUSES, "");
    assert_eq!(findings("reuses", "", PARITY, &files), []);
    let files = files.without(IMPLEMENTATION);
    assert_eq!(reuses(&files), []);
}

#[test]
fn reuses_missing_no_field_a_repeated_field_and_a_vague_reason() {
    let files = tree().edit(IMPLEMENTATION, &format!("{REUSES}\n"), "");
    assert_eq!(
        reuses(&files),
        [seen(
            "reuses-missing",
            IMPLEMENTATION,
            Some(9),
            &format!(
                "{PREFIX} is open but has no Reuses field; name the catalog keys of {CATALOG} it reuses, or write `none; <why no catalog entry applies>`"
            )
        )]
    );

    let files = tree().edit(IMPLEMENTATION, REUSES, &format!("{REUSES}\n{REUSES}"));
    assert_eq!(
        reuses(&files),
        [seen(
            "reuses-missing",
            IMPLEMENTATION,
            Some(13),
            &format!("{PREFIX} repeats its Reuses field; keep one")
        )]
    );

    for vague in ["none; not applicable.", "none; too small here."] {
        let files = tree().edit(IMPLEMENTATION, REUSES, &format!("  - Reuses: {vague}"));
        assert_eq!(
            reuses(&files),
            [seen(
                "reuses-missing",
                IMPLEMENTATION,
                Some(12),
                &format!(
                    "{PREFIX} gives no specific reason for reusing nothing; say why no entry of {CATALOG} applies, in at least 4 words"
                )
            )],
            "{vague}"
        );
    }

    let files = tree().edit(IMPLEMENTATION, REUSES, "  - Reuses: the parser");
    assert_eq!(
        reuses(&files),
        [seen(
            "reuses-missing",
            IMPLEMENTATION,
            Some(12),
            &format!(
                "{PREFIX} has the Reuses value \"the parser\"; write backticked catalog keys separated by commas, or `none; <reason>`"
            )
        )]
    );
}

#[test]
fn reuses_unknown_and_a_key_named_twice() {
    let files = tree().edit(
        IMPLEMENTATION,
        REUSES,
        "  - Reuses: `shared-parser`, `nope`, `shared-parser`",
    );
    assert_eq!(
        reuses(&files),
        [
            seen(
                "reuses-unknown",
                IMPLEMENTATION,
                Some(12),
                &format!("{PREFIX} reuses nope, which {CATALOG} does not list")
            ),
            seen(
                "reuses-missing",
                IMPLEMENTATION,
                Some(12),
                &format!("{PREFIX} names shared-parser twice in its Reuses field")
            ),
        ]
    );
}

#[test]
fn reuses_planner_a_sole_planner_names_its_key_and_a_co_planner_does_not() {
    let files = tree().edit(IMPLEMENTATION, REUSES, "  - Reuses: `event-bus`");
    assert_eq!(
        reuses(&files),
        [seen(
            "reuses-planner",
            IMPLEMENTATION,
            Some(12),
            &format!(
                "{PREFIX} names event-bus, which it alone plans in {CATALOG}; it builds the mechanism, so drop the key"
            )
        )]
    );

    let files = tree()
        .edit(
            CATALOG,
            "  - Planned by: `I-BASE-002`",
            "  - Planned by: `I-BASE-002`, `I-BASE-003`",
        )
        .edit(
            IMPLEMENTATION,
            REUSES,
            &format!(
                "{REUSES}\n- `I-BASE-003`: More\n  - Status: `planned`\n  - Reuses: `event-bus`"
            ),
        );
    assert_eq!(
        reuses(&files),
        [seen(
            "reuses-planner",
            IMPLEMENTATION,
            Some(12),
            &format!(
                "{PREFIX} is one of the records {CATALOG} names as planning event-bus; name the key, since whichever lands first builds it"
            )
        )]
    );
}

#[test]
fn reuses_planner_drop_and_name_have_keys_of_their_own() {
    // Opposite violations of one record and key, on one line: a baseline entry for one must not
    // accept the other, and the key is what tells them apart in the fingerprint.
    let drop = tree().edit(IMPLEMENTATION, REUSES, "  - Reuses: `event-bus`");
    let name = tree()
        .edit(
            CATALOG,
            "  - Planned by: `I-BASE-002`",
            "  - Planned by: `I-BASE-002`, `I-BASE-003`",
        )
        .edit(
            IMPLEMENTATION,
            REUSES,
            &format!(
                "{REUSES}\n- `I-BASE-003`: More\n  - Status: `planned`\n  - Reuses: `event-bus`"
            ),
        );
    let keyed = |key: &str| {
        (
            "reuses-planner".to_owned(),
            IMPLEMENTATION.to_owned(),
            Some(12),
            Some(key.to_owned()),
        )
    };
    assert_eq!(
        keys("reuses", OPTIONS, PARITY, &drop),
        [keyed("I-BASE-002/event-bus#drop")]
    );
    assert_eq!(
        keys("reuses", OPTIONS, PARITY, &name),
        [keyed("I-BASE-002/event-bus#name")]
    );
}

#[test]
fn catalog_entry_problems_are_one_finding_per_entry() {
    let files = tree().with(
        CATALOG,
        "- `Bad_Key`: x\n\
         \x20 - Decisions: `D-TWO-001`, `D-NONE-001`\n\
         \x20 - Path: `src/gone.ts`\n\
         \x20 - Planned by: `I-ALPHA-ONE-001`\n\
         - `event-bus`: y\n\
         \x20 - Decisions: none\n\
         \x20 - Planned by: `I-ALPHA-ONE-001`\n\
         - `event-bus`: z\n\
         \x20 - Path: `src/parser.ts`\n",
    );
    assert_eq!(
        reuses(&files),
        [
            seen(
                "catalog-entry",
                CATALOG,
                Some(1),
                "the shared mechanism Bad_Key: its key is not lowercase kebab-case; D-TWO-001 is not an accepted decision; D-NONE-001 is not an accepted decision; it needs exactly one Path or Planned by field"
            ),
            seen(
                "catalog-entry",
                CATALOG,
                Some(5),
                "the shared mechanism event-bus: its Decisions name no decision; I-ALPHA-ONE-001 is no planned or in-progress implementation record; once the mechanism lands, name its Path"
            ),
            seen(
                "catalog-entry",
                CATALOG,
                Some(8),
                "the shared mechanism event-bus: it needs exactly one Decisions field; its key repeats an earlier entry"
            ),
        ]
    );

    let files = tree().edit(
        CATALOG,
        "`src/parser.ts`, `src/parser/`",
        "`src/gone.ts`, `src/gone/`",
    );
    assert_eq!(
        reuses(&files),
        [seen(
            "catalog-entry",
            CATALOG,
            Some(3),
            "the shared mechanism shared-parser: src/gone.ts is not tracked; src/gone/ is not tracked"
        )]
    );
}

#[test]
fn without_a_decision_index_every_decision_passes() {
    let files = tree()
        .edit(
            CATALOG,
            "  - Decisions: `D-ONE-001`\n  - Path",
            "  - Decisions: `D-NONE-001`\n  - Path",
        )
        .without("docs/ledger/decision-index.md");
    assert_eq!(reuses(&files), []);
}

#[test]
fn a_missing_catalog_is_reported() {
    let files = tree().without(CATALOG);
    assert_eq!(
        reuses(&files),
        [seen(
            "catalog-entry",
            CATALOG,
            None,
            "the catalog of shared mechanisms is missing; the Reuses fields name its keys"
        ),]
    );
}

#[test]
fn excluded_directories_and_the_reason_options() {
    let files = tree()
        .edit(IMPLEMENTATION, &format!("{REUSES}\n"), "")
        .with(
            "docs/ledger/implementation-slices/mirror/a.md",
            "- `I-MIRROR-001`: Mirrored\n  - Status: `planned`\n",
        );
    assert_eq!(reuses(&files).len(), 2);
    let excluded = r#"{"catalog":"docs/reference/mechanisms.md","exclude":["docs/ledger/implementation-slices/mirror"]}"#;
    assert_eq!(findings("reuses", excluded, PARITY, &files).len(), 1);

    let files = tree().edit(IMPLEMENTATION, REUSES, "  - Reuses: none; too small here.");
    let lenient = r#"{"catalog":"docs/reference/mechanisms.md","reason-words":3}"#;
    assert_eq!(findings("reuses", lenient, PARITY, &files), []);
    let strict = r#"{"catalog":"docs/reference/mechanisms.md","reason-words":3,"generic-reasons":"^too small"}"#;
    assert_eq!(findings("reuses", strict, PARITY, &files).len(), 1);

    let message = error(
        "reuses",
        r#"{"catalog":"docs/reference/mechanisms.md","generic-reasons":"("}"#,
        PARITY,
        &files,
    );
    assert!(message.contains("generic-reasons"), "{message}");
}

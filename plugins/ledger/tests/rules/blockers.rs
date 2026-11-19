//! `ledger/blockers`.

use crate::{FOLLOW_UPS, IMPLEMENTATION, PARITY, PLAN, findings, ledger, seen};

const BASE_BLOCKED: &str = "  - Blocked by: none\n  - Reuses";

#[test]
fn open_blockers_pass() {
    assert_eq!(findings("blockers", "", PARITY, &ledger()), []);
    let files = ledger().edit(
        IMPLEMENTATION,
        BASE_BLOCKED,
        "  - Blocked by: `I-NOPE-001` and `F-TWO`\n  - Reuses",
    );
    assert_eq!(findings("blockers", "", PARITY, &files), []);
}

#[test]
fn closed_blocker_a_planned_record_waits_for_a_closed_one() {
    let files = ledger()
        .edit(
            IMPLEMENTATION,
            BASE_BLOCKED,
            "  - Blocked by: `I-ALPHA-ONE-001` until `F-ONE` lands; `I-ALPHA-ONE-001` again\n  - Reuses",
        )
        .edit(FOLLOW_UPS, "Status `done`", "Status `completed and mirrored`");
    assert_eq!(
        findings("blockers", "", PARITY, &files),
        [
            seen(
                "closed-blocker",
                IMPLEMENTATION,
                Some(9),
                "docs/ledger/implementation-slices.md I-BASE-002 is planned, but its Blocked by names I-ALPHA-ONE-001, which is `completed`; keep only open blockers there and move the context to a Dependency context field"
            ),
            seen(
                "closed-blocker",
                IMPLEMENTATION,
                Some(9),
                "docs/ledger/implementation-slices.md I-BASE-002 is planned, but its Blocked by names F-ONE, which is `completed and mirrored`; keep only open blockers there and move the context to a Dependency context field"
            ),
        ]
    );
}

#[test]
fn started_blocker_a_started_record_names_a_blocker() {
    let files = ledger()
        .edit(IMPLEMENTATION, "`planned`", "`in progress`")
        .edit(
            IMPLEMENTATION,
            BASE_BLOCKED,
            "  - Blocked by: `F-ONE`\n  - Reuses",
        );
    assert_eq!(
        findings("blockers", "", PARITY, &files),
        [seen(
            "started-blocker",
            IMPLEMENTATION,
            Some(9),
            "docs/ledger/implementation-slices.md I-BASE-002 is in progress, so its Blocked by must be none; record the remaining wait as a dated Completion dependency"
        )]
    );
}

#[test]
fn plan_headings_are_records_with_parity() {
    let files = ledger().edit(PLAN, "`completed`", "`planned`").edit(
        PLAN,
        "- **Blocked by:** none",
        "- **Blocked by:** `F-ONE`",
    );
    assert_eq!(
        findings("blockers", "", PARITY, &files),
        [seen(
            "closed-blocker",
            PLAN,
            Some(3),
            "docs/ledger/plans/alpha.md I-ALPHA-ONE-001 is planned, but its Blocked by names F-ONE, which is `done`; keep only open blockers there and move the context to a Dependency context field"
        )]
    );
    assert_eq!(findings("blockers", "", "", &files), []);
}

#[test]
fn the_prefixes_and_closed_statuses_are_options() {
    let files = ledger().edit(IMPLEMENTATION, "`planned`", "`queued`").edit(
        IMPLEMENTATION,
        BASE_BLOCKED,
        "  - Blocked by: `F-ONE`\n  - Reuses",
    );
    assert_eq!(findings("blockers", "", PARITY, &files), []);
    assert_eq!(
        findings("blockers", "", r#"{"planned-prefixes":["queued"]}"#, &files).len(),
        1
    );
    assert_eq!(
        findings(
            "blockers",
            "",
            r#"{"planned-prefixes":["queued"],"closed-statuses":["finished"]}"#,
            &files
        ),
        []
    );
}

#[test]
fn a_record_in_the_same_stem_directory_belongs_to_its_collection() {
    let files = ledger()
        .with(
            "docs/ledger/implementation-slices/more.md",
            "# More\n\n- `I-MORE-003`: More work\n  - Status: `completed`\n",
        )
        .edit(
            IMPLEMENTATION,
            BASE_BLOCKED,
            "  - Blocked by: `I-MORE-003`\n  - Reuses",
        );
    assert_eq!(
        findings("blockers", "", PARITY, &files),
        [seen(
            "closed-blocker",
            IMPLEMENTATION,
            Some(9),
            "docs/ledger/implementation-slices.md I-BASE-002 is planned, but its Blocked by names I-MORE-003, which is `completed`; keep only open blockers there and move the context to a Dependency context field"
        )]
    );
}

#[test]
fn a_record_inside_fenced_code_is_not_a_record() {
    let blocked = ledger().edit(
        IMPLEMENTATION,
        BASE_BLOCKED,
        "  - Blocked by: `I-ALPHA-ONE-001`\n  - Reuses",
    );
    assert_eq!(findings("blockers", "", PARITY, &blocked).len(), 1);

    // An open I-ALPHA-ONE-001 before the completed one would win as the first definition, were it
    // a record.
    let example = "- `I-ALPHA-ONE-001`: An example\n  - Status: `planned`\n";
    let fenced = blocked.clone().edit(
        IMPLEMENTATION,
        "# Implementation\n",
        &format!("# Implementation\n\n```markdown\n{example}```\n"),
    );
    assert_eq!(findings("blockers", "", PARITY, &fenced).len(), 1);
    let unfenced = blocked.edit(
        IMPLEMENTATION,
        "# Implementation\n",
        &format!("# Implementation\n\n{example}"),
    );
    assert_eq!(findings("blockers", "", PARITY, &unfenced), []);
}

#[test]
fn a_ledger_of_another_project_is_read_through_its_relative_paths() {
    let shared = ledger()
        .edit(
            IMPLEMENTATION,
            BASE_BLOCKED,
            "  - Blocked by: `F-ONE`\n  - Reuses",
        )
        .renamed("../shared/");
    let options = r#"{"implementation":"../shared/docs/ledger/implementation-slices.md","verification":"../shared/docs/ledger/verification.md","follow-ups":"../shared/docs/ledger/follow-ups.md","blockers":"../shared/docs/ledger/production-blockers.md","decisions":"../shared/docs/ledger/decision-index.md"}"#;
    assert_eq!(
        findings("blockers", "", options, &shared),
        [seen(
            "closed-blocker",
            "../shared/docs/ledger/implementation-slices.md",
            Some(9),
            "../shared/docs/ledger/implementation-slices.md I-BASE-002 is planned, but its Blocked by names F-ONE, which is `done`; keep only open blockers there and move the context to a Dependency context field"
        )]
    );
    assert_eq!(
        findings("blockers", "", "", &shared),
        [],
        "the default paths name nothing in this project"
    );
}

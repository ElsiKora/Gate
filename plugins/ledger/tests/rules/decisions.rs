//! `ledger/decisions`.

use crate::{Files, findings, seen};

const INDEX: &str = "docs/ledger/decision-index.md";
const COVERAGE: &str = "docs/adr/decision-coverage.md";
const ADR: &str = "docs/adr/0001-one.md";

const ADR_TEXT: &str = "# 0001 One\n\
                        \n\
                        - Linked ledger IDs: `D-ONE-001`\n\
                        \n\
                        ## Context\n\
                        ## Decision\n\
                        ## Consequences\n\
                        ## Alternatives Considered\n\
                        ## Verification\n";

fn tree() -> Files {
    Files::new(&[
        (
            INDEX,
            "# Decisions\n\
             \n\
             | ID | Status | Title | File |\n\
             | --- | --- | --- | --- |\n\
             | `D-ONE-001` | `accepted` | One | `decisions/one.md` |\n",
        ),
        (
            COVERAGE,
            "# Coverage\n\
             \n\
             | `D-ONE-001` | `accepted` | `0001-one.md` | `decisions/one.md` |\n",
        ),
        ("docs/ledger/decisions/one.md", "- `D-ONE-001`: one\n"),
        (ADR, ADR_TEXT),
        ("docs/adr/0000-template.md", "# Template\n"),
    ])
}

fn decisions(files: &Files) -> Vec<crate::Seen> {
    findings("decisions", "", "", files)
}

#[test]
fn an_agreeing_index_coverage_and_adr_pass() {
    assert_eq!(decisions(&tree()), []);
}

#[test]
fn the_rule_is_off_while_the_coverage_is_untracked() {
    let files = tree().without(COVERAGE).without(ADR);
    assert_eq!(decisions(&files), []);
}

#[test]
fn adr_sections_and_linked_ids() {
    let files = tree()
        .edit(ADR, "## Verification\n", "")
        .edit(ADR, "## Context\n", "## Context\n## Context\n")
        .edit(ADR, "- Linked ledger IDs: `D-ONE-001`\n", "");
    assert_eq!(
        decisions(&files),
        [
            seen(
                "adr-sections",
                ADR,
                None,
                "docs/adr/0001-one.md must contain exactly one ## Context"
            ),
            seen(
                "adr-sections",
                ADR,
                None,
                "docs/adr/0001-one.md must contain exactly one ## Verification"
            ),
            seen(
                "adr-linked-ids",
                ADR,
                None,
                "docs/adr/0001-one.md must link ledger decision IDs"
            ),
            seen(
                "decision-coverage",
                ADR,
                None,
                "docs/adr/0001-one.md must link D-ONE-001"
            ),
        ]
    );
}

#[test]
fn a_section_inside_a_fence_does_not_count() {
    let files = tree().edit(ADR, "## Verification\n", "```\n## Verification\n```\n");
    assert_eq!(decisions(&files).len(), 1);
}

#[test]
fn decision_coverage_a_row_missing_on_one_side_and_a_disagreement() {
    let files = tree().edit(
        INDEX,
        "| `D-ONE-001` | `accepted` | One | `decisions/one.md` |\n",
        "| `D-ONE-001` | `proposed` | One | `decisions/one.md` |\n| `D-TWO-001` | `accepted` | Two | `decisions/one.md` |\n",
    );
    assert_eq!(
        decisions(&files),
        [
            seen(
                "decision-coverage",
                COVERAGE,
                Some(3),
                "D-ONE-001 has inconsistent status or canonical file in decision index/ADR coverage"
            ),
            seen(
                "decision-coverage",
                COVERAGE,
                None,
                "docs/adr/decision-coverage.md is missing decision D-TWO-001"
            ),
        ]
    );
}

#[test]
fn decision_coverage_a_missing_adr_and_canonical_file() {
    let files = tree().without("docs/ledger/decisions/one.md").edit(
        COVERAGE,
        "`0001-one.md`",
        "`0001-one.md`, `0002-two.md`",
    );
    assert_eq!(
        decisions(&files),
        [
            seen(
                "decision-canonical-file",
                COVERAGE,
                Some(3),
                "D-ONE-001 references missing canonical file decisions/one.md"
            ),
            seen(
                "decision-coverage",
                COVERAGE,
                Some(3),
                "D-ONE-001 references missing ADR docs/adr/0002-two.md"
            ),
        ]
    );
}

#[test]
fn record_shape_status_and_unique() {
    let files = tree().edit(
        INDEX,
        "| `D-ONE-001` | `accepted` | One | `decisions/one.md` |\n",
        "| `D-ONE-001` | `accepted` | One | `decisions/one.md` |\n| `D-ONE-001` | `maybe` | One | `decisions/one.md` |\n| `D-BAD` | `accepted` | `decisions/one.md` |\n",
    );
    assert_eq!(
        decisions(&files),
        [
            seen(
                "decision-status",
                INDEX,
                Some(6),
                "docs/ledger/decision-index.md contains invalid decision status maybe for D-ONE-001"
            ),
            seen(
                "record-unique",
                INDEX,
                Some(6),
                "docs/ledger/decision-index.md contains duplicate decision D-ONE-001"
            ),
            seen(
                "record-shape",
                INDEX,
                Some(7),
                "docs/ledger/decision-index.md contains malformed decision row: | `D-BAD` | `accepted` | `decisions/one.md` |"
            ),
            seen(
                "decision-coverage",
                COVERAGE,
                Some(3),
                "D-ONE-001 has inconsistent status or canonical file in decision index/ADR coverage"
            ),
        ]
    );
}

#[test]
fn the_paths_sections_and_statuses_are_options() {
    let files = Files::new(&[
        ("ledger/index.md", "| `D-A-001` | `open` | A | `a.md` |\n"),
        (
            "adr/coverage.md",
            "| `D-A-001` | `open` | `0001-a.md` | `a.md` |\n",
        ),
        ("records/a.md", "a\n"),
        ("adr/0001-a.md", "Links: `D-A-001`\n# Why\n"),
    ]);
    let options = r##"{"ledger-root":"records","adr-directory":"adr","coverage":"adr/coverage.md","adr-sections":["# Why"],"statuses":["open"],"linked-ids-prefix":"Links:"}"##;
    assert_eq!(
        findings(
            "decisions",
            options,
            r#"{"decisions":"ledger/index.md"}"#,
            &files
        ),
        []
    );
    assert_eq!(
        findings("decisions", options, "", &files),
        [seen(
            "decision-coverage",
            "docs/ledger/decision-index.md",
            None,
            "docs/ledger/decision-index.md is missing decision D-A-001"
        )]
    );
}

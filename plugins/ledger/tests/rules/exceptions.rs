//! `ledger/exception-owners` and `ledger/exception-decisions`.

use crate::{Files, error, findings, ledger, seen};

const EXCEPTIONS: &str = "gate.exceptions.toml";
const INDEX: &str = "docs/ledger/decision-index.md";

fn exception(rule: &str, extra: &str) -> String {
    format!(
        "[[exception]]\nrule = \"{rule}\"\ntarget = \"src/a.ts\"\nreason = \"kept for now\"\n{extra}\n"
    )
}

fn tree(exceptions: &str) -> Files {
    ledger().with(EXCEPTIONS, exceptions).with(
        INDEX,
        "| `D-ONE-001` | `accepted` | One | `one.md` |\n| `D-TWO-001` | `proposed` | Two | `two.md` |\n",
    )
}

#[test]
fn an_open_owner_and_an_accepted_decision_pass() {
    let files = tree(&exception(
        "demo/rule",
        "owner = \"I-BASE-002\"\ndecision = \"D-ONE-001\"",
    ));
    assert_eq!(findings("exception-owners", "", "", &files), []);
    assert_eq!(findings("exception-decisions", "", "", &files), []);
    let files = ledger();
    assert_eq!(findings("exception-owners", "", "", &files), []);
    assert_eq!(findings("exception-decisions", "", "", &files), []);
}

#[test]
fn exception_owners_an_unknown_and_a_completed_owner() {
    let files = tree(&format!(
        "{}\n{}\n{}",
        exception("demo/rule", ""),
        exception("demo/rule#part", "owner = \"I-NOPE-001\""),
        exception("demo/other", "owner = \"I-ALPHA-ONE-001\""),
    ));
    assert_eq!(
        findings("exception-owners", "", "", &files),
        [
            seen(
                "",
                EXCEPTIONS,
                Some(7),
                "the exception for demo/rule#part names the owner I-NOPE-001, which docs/ledger/implementation-slices.md does not list; name an open implementation record, or remove the owner"
            ),
            seen(
                "",
                EXCEPTIONS,
                Some(13),
                "the exception for demo/other names the owner I-ALPHA-ONE-001, which is `completed`; only an open record, planned or in progress, owns an exception, so fix the finding, or move the exception to the open record that owns the work"
            ),
        ]
    );
}

#[test]
fn only_a_planned_or_in_progress_owner_is_open() {
    let records = "- `I-NONE-003`: No status\n\
                   \x20 - Blocked by: none\n\
                   - `I-BLOCKED-004`: Blocked work\n\
                   \x20 - Status: `blocked`\n\
                   - `I-REJECTED-005`: Rejected work\n\
                   \x20 - Status: `rejected`\n\
                   - `I-STARTED-006`: Started work\n\
                   \x20 - Status: `in progress`\n";
    let owners = [
        "I-BASE-002",
        "I-NONE-003",
        "I-BLOCKED-004",
        "I-REJECTED-005",
        "I-STARTED-006",
    ]
    .map(|owner| exception("demo/rule", &format!("owner = \"{owner}\"")))
    .join("\n");
    let files = tree(&owners).edit(
        crate::IMPLEMENTATION,
        "builds the first parser itself.\n",
        &format!("builds the first parser itself.\n{records}"),
    );
    let reported = |owner: &str, state: &str, line: u32| {
        seen(
            "",
            EXCEPTIONS,
            Some(line),
            &format!(
                "the exception for demo/rule names the owner {owner}, {state}; only an open record, planned or in progress, owns an exception, so fix the finding, or move the exception to the open record that owns the work"
            ),
        )
    };
    assert_eq!(
        findings("exception-owners", "", "", &files),
        [
            reported("I-NONE-003", "which has no status", 7),
            reported("I-BLOCKED-004", "which is `blocked`", 13),
            reported("I-REJECTED-005", "which is `rejected`", 19),
        ],
        "I-BASE-002 is planned and I-STARTED-006 in progress: both pass"
    );
}

#[test]
fn exception_decisions_unknown_and_unaccepted() {
    let files = tree(&format!(
        "{}\n{}\n{}",
        exception("demo/rule", ""),
        exception("demo/rule", "decision = \"D-TWO-001\""),
        exception("demo/rule", "decision = \"D-NOPE-001\""),
    ));
    assert_eq!(
        findings("exception-decisions", "", "", &files),
        [
            seen(
                "",
                EXCEPTIONS,
                Some(7),
                "the exception for demo/rule names the decision D-TWO-001, which is `proposed`; an exception needs an accepted decision"
            ),
            seen(
                "",
                EXCEPTIONS,
                Some(13),
                "the exception for demo/rule names the decision D-NOPE-001, which docs/ledger/decision-index.md does not list; name an accepted decision of the index"
            ),
        ]
    );

    // A missing decision is the host's to refuse under `[decision-policy] require-decision`.
    let files = files.with("gate.toml", "[decision-policy]\nrequire-decision = true\n");
    assert_eq!(findings("exception-decisions", "", "", &files).len(), 2);
}

#[test]
fn the_exceptions_file_is_an_option() {
    let files = ledger().with(
        "config/gate.exceptions.toml",
        &exception("demo/rule", "owner = \"I-NOPE-001\""),
    );
    assert_eq!(findings("exception-owners", "", "", &files), []);
    let found = findings(
        "exception-owners",
        r#"{"exceptions":"config/gate.exceptions.toml"}"#,
        "",
        &files,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1, "config/gate.exceptions.toml");
}

#[test]
fn an_unreadable_exceptions_file_is_an_error() {
    let message = error("exception-decisions", "", "", &tree("[[exception]\n"));
    assert!(
        message.contains("gate.exceptions.toml is not a valid exceptions file"),
        "{message}"
    );
}

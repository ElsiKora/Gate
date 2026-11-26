//! `ledger/exception-owners`: one repository-scope rule over every `gate.exceptions.toml` the
//! repository call lists, the records read from the repository root.

use gate_sdk::RuleDecision;

use crate::{Files, IMPLEMENTATION, findings, findings_listed, ledger, listed, run_listed, seen};

const EXCEPTIONS: &str = "gate.exceptions.toml";
const NESTED: &str = "apps/one/gate.exceptions.toml";
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

/// The findings of the rule in a repository call that lists `listed`.
fn owners(listed: &[RuleDecision], files: &Files) -> Vec<crate::Seen> {
    findings_listed("exception-owners", "", listed, files)
}

/// The listing of the root exceptions file of [`tree`]: one entry per exception, at lines 1, 7, …
fn root(count: u32) -> Vec<RuleDecision> {
    (0..count)
        .map(|index| listed(EXCEPTIONS, "demo/rule", 1 + 6 * index, ""))
        .collect()
}

const UNLISTED: &str = "which docs/ledger/implementation-slices.md does not list; name an open implementation record, or remove the owner";

#[test]
fn an_open_owner_passes_and_a_decision_is_not_this_rules() {
    // A proposed decision beside an open owner: the decision is `ledger/config-decisions`'s.
    let files = tree(&exception(
        "demo/rule",
        "owner = \"I-BASE-002\"\ndecision = \"D-TWO-001\"",
    ));
    assert_eq!(
        owners(&[listed(EXCEPTIONS, "demo/rule", 1, "D-TWO-001")], &files),
        []
    );
    assert_eq!(owners(&[], &ledger()), []);
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
        owners(&root(3), &files),
        [
            seen(
                "",
                EXCEPTIONS,
                Some(7),
                &format!("the exception for demo/rule#part names the owner I-NOPE-001, {UNLISTED}")
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
    let owners_text = [
        "I-BASE-002",
        "I-NONE-003",
        "I-BLOCKED-004",
        "I-REJECTED-005",
        "I-STARTED-006",
    ]
    .map(|owner| exception("demo/rule", &format!("owner = \"{owner}\"")))
    .join("\n");
    let files = tree(&owners_text).edit(
        IMPLEMENTATION,
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
        owners(&root(5), &files),
        [
            reported("I-NONE-003", "which has no status", 7),
            reported("I-BLOCKED-004", "which is `blocked`", 13),
            reported("I-REJECTED-005", "which is `rejected`", 19),
        ],
        "I-BASE-002 is planned and I-STARTED-006 in progress: both pass"
    );
}

/// A root exceptions file with an open owner (line 1) and an undecided unknown owner (line 7), and
/// a nested project's with an open owner (line 1) and an unknown one (line 7). The nested project
/// keeps a records file of its own that lists the unknown owner: it is not the records.
fn nested() -> Files {
    tree(&format!(
        "{}\n{}",
        exception(
            "demo/rule",
            "owner = \"I-BASE-002\"\ndecision = \"D-ONE-001\""
        ),
        exception("demo/other", "owner = \"I-ROOT-009\""),
    ))
    .with(
        NESTED,
        &format!(
            "{}\n{}",
            exception("demo/rule", "owner = \"I-BASE-002\""),
            exception(
                "demo/rule",
                "owner = \"I-NESTED-009\"\ndecision = \"D-ONE-001\""
            ),
        ),
    )
    .with(
        &format!("apps/one/{IMPLEMENTATION}"),
        "# Implementation\n\n- `I-NESTED-009`: Nested work\n  - Status: `planned`\n",
    )
}

/// What the core lists for [`nested`]: every exception, root first, undecided ones with an empty
/// decision.
fn nested_listing() -> Vec<RuleDecision> {
    vec![
        listed(EXCEPTIONS, "demo/rule", 1, "D-ONE-001"),
        listed(EXCEPTIONS, "demo/other", 7, ""),
        listed(NESTED, "demo/rule", 1, ""),
        listed(NESTED, "demo/rule", 7, "D-ONE-001"),
    ]
}

#[test]
fn every_listed_file_is_judged_once_against_the_root_records() {
    let expected = [
        seen(
            "",
            EXCEPTIONS,
            Some(7),
            &format!("the exception for demo/other names the owner I-ROOT-009, {UNLISTED}"),
        ),
        seen(
            "",
            NESTED,
            Some(7),
            &format!("the exception for demo/rule names the owner I-NESTED-009, {UNLISTED}"),
        ),
    ];
    assert_eq!(owners(&nested_listing(), &nested()), expected);

    // A file listed once per exception, and rule entries of gate.toml files beside them, are each
    // read once: nothing is reported twice, and a gate.toml is never read as exceptions.
    let mut listing = nested_listing();
    listing.extend(nested_listing());
    listing.push(listed("gate.toml", "demo/rule", 12, "D-ONE-001"));
    listing.push(listed("apps/one/gate.toml", "demo/*", 5, "D-ONE-001"));
    assert_eq!(owners(&listing, &nested()), expected);
}

#[test]
fn the_records_are_the_repository_calls_option_relative_to_the_repository_root() {
    // Pointing the option at the nested project's records makes its owner known and every other
    // owner unknown, in both files: the path is the repository's, whichever file the exception is
    // in.
    let records = format!("apps/one/{IMPLEMENTATION}");
    let found = findings_listed(
        "exception-owners",
        &format!(r#"{{"implementation":"{records}"}}"#),
        &nested_listing(),
        &nested(),
    );
    let unlisted = |path: &str, line: u32, rule: &str, owner: &str| {
        seen(
            "",
            path,
            Some(line),
            &format!(
                "the exception for {rule} names the owner {owner}, which {records} does not list; name an open implementation record, or remove the owner"
            ),
        )
    };
    assert_eq!(
        found,
        [
            unlisted(EXCEPTIONS, 1, "demo/rule", "I-BASE-002"),
            unlisted(EXCEPTIONS, 7, "demo/other", "I-ROOT-009"),
            unlisted(NESTED, 1, "demo/rule", "I-BASE-002"),
        ]
    );
}

#[test]
fn a_project_call_and_an_unlisted_file_judge_nothing() {
    // A project's call lists no exception, so the rule reads no file there; in the repository
    // call a file the core did not list (an ignored fixture tree, say) is never read.
    let files = nested().with(
        "fixtures/gate.exceptions.toml",
        &exception("demo/rule", "owner = \"I-NOPE-001\""),
    );
    assert_eq!(findings("exception-owners", "", "", &files), []);
    assert_eq!(owners(&nested_listing(), &files).len(), 2);
}

#[test]
fn a_root_alone_is_judged_as_any_file() {
    // The first consumer's layout: a root project with the only gate.exceptions.toml at the root,
    // and the same layout at a root that is not a project. The repository call lists the root
    // file either way, so the result is the same.
    let files = tree(&format!(
        "{}\n{}\n{}",
        exception("demo/rule", "owner = \"I-NOPE-001\""),
        exception("demo/rule", "decision = \"D-TWO-001\""),
        exception("demo/rule", "owner = \"I-BASE-002\""),
    ));
    assert_eq!(
        owners(&root(3), &files),
        [seen(
            "",
            EXCEPTIONS,
            Some(1),
            &format!("the exception for demo/rule names the owner I-NOPE-001, {UNLISTED}")
        )]
    );
}

#[test]
fn an_unreadable_exceptions_file_is_an_error() {
    let message = run_listed(
        "exception-owners",
        "",
        "",
        &root(1),
        &tree("[[exception]\n"),
    )
    .expect_err("the run fails")
    .message()
    .to_owned();
    assert!(
        message.contains("gate.exceptions.toml is not a valid exceptions file"),
        "{message}"
    );
}

#[test]
fn the_rule_takes_no_options() {
    let message = run_listed(
        "exception-owners",
        r#"{"exceptions":"config/gate.exceptions.toml"}"#,
        "",
        &root(1),
        &tree(&exception("demo/rule", "")),
    )
    .expect_err("the run fails")
    .message()
    .to_owned();
    assert!(message.contains("exceptions"), "{message}");
}

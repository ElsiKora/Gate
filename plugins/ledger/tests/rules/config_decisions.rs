//! `ledger/config-decisions`: the decisions come from the repository call, never from `gate.toml`
//! or `gate.exceptions.toml`, each at the file and line that writes it.

use gate_sdk::{CheckRequest, FileLocation, Finding, Location, RuleDecision, RuleRun, Span};

use crate::{Files, ledger};

const INDEX: &str = "docs/ledger/decision-index.md";

fn tree() -> Files {
    ledger().with(
        INDEX,
        "| `D-ONE-001` | `accepted` | One | `one.md` |\n| `D-TWO-001` | `proposed` | Two | `two.md` |\n| `D-API-002` | `proposed` | Api | `api.md` |\n",
    )
}

/// A decision written on the entry `key` at `path`:`line`.
fn written(path: &str, key: &str, id: &str, line: u32) -> RuleDecision {
    RuleDecision {
        key: key.to_owned(),
        decision: id.to_owned(),
        at: FileLocation {
            path: path.to_owned(),
            span: Some(Span::line(line)),
        },
    }
}

/// A decision on a rule entry of the root `gate.toml`.
fn decision(key: &str, id: &str, line: u32) -> RuleDecision {
    written("gate.toml", key, id, line)
}

/// Path, line, message and key of each finding of the rule, in a repository call carrying
/// `decisions`.
fn reported(
    decisions: Vec<RuleDecision>,
    plugin_options: &str,
    files: &Files,
) -> Vec<(String, Option<u32>, String, Option<String>)> {
    reported_with(decisions, plugin_options, "gate.toml", files)
}

/// As [`reported`], with the repository call's own `config_file` (empty under a root project).
fn reported_with(
    decisions: Vec<RuleDecision>,
    plugin_options: &str,
    config_file: &str,
    files: &Files,
) -> Vec<(String, Option<u32>, String, Option<String>)> {
    let request = CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: "config-decisions".to_owned(),
            options: String::new(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: plugin_options.to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: decisions,
        config_file: config_file.to_owned(),
    };
    gate_sdk::check(&ledger::PLUGIN, files, &request)
        .expect("the rule runs")
        .into_iter()
        .map(|finding: Finding| {
            let Some(Location::File(location)) = finding.location else {
                panic!("every finding names a file: {:?}", finding.location);
            };
            (
                location.path,
                location.span.map(|span| span.line),
                finding.message,
                finding.key,
            )
        })
        .collect()
}

#[test]
fn an_accepted_decision_of_a_rule_and_of_a_family_passes() {
    let files = tree();
    let decisions = vec![
        decision("demo/rule", "D-ONE-001", 4),
        decision("demo/*", "D-ONE-001", 5),
    ];
    assert_eq!(reported(decisions, "", &files), []);
}

#[test]
fn a_call_without_decisions_reads_no_index() {
    // No index, and an index option that does not exist: nothing to check, nothing reported.
    let files = ledger();
    assert_eq!(reported(Vec::new(), "", &files), []);
    assert_eq!(
        reported(Vec::new(), r#"{"decisions":"no/such.md"}"#, &files),
        []
    );
}

#[test]
fn an_unlisted_and_an_unaccepted_decision_are_reported_at_their_entries() {
    let files = tree();
    let decisions = vec![
        decision("demo/rule", "D-TWO-001", 4),
        decision("demo/*", "D-NOPE-001", 7),
        decision("demo/other", "D-ONE-001", 9),
    ];
    assert_eq!(
        reported(decisions, "", &files),
        [
            (
                "gate.toml".to_owned(),
                Some(4),
                "the entry for \"demo/rule\" names the decision D-TWO-001, which is `proposed`; an entry needs an accepted decision".to_owned(),
                Some("demo/rule/D-TWO-001#not-accepted".to_owned()),
            ),
            (
                "gate.toml".to_owned(),
                Some(7),
                "the entry for \"demo/*\" names the decision D-NOPE-001, which docs/ledger/decision-index.md does not list; name an accepted decision of the index".to_owned(),
                Some("demo/*/D-NOPE-001#unlisted".to_owned()),
            ),
        ]
    );
}

#[test]
fn a_nested_projects_entries_and_exceptions_are_judged_at_their_own_files() {
    // The root is a project (the repository call has no file of its own) and `infra/cdk` is
    // nested in it. Every file's decisions arrive in the one repository call; the index is the
    // root's, relative to the repository root, and each finding points at the file that wrote it.
    let files = tree();
    let decisions = vec![
        decision("demo/rule", "D-ONE-001", 4),
        written("infra/cdk/gate.toml", "demo/rule", "D-ONE-001", 6),
        written("infra/cdk/gate.toml", "demo/*", "D-NOPE-003", 7),
        written("infra/cdk/gate.toml", "demo/other", "D-TWO-001", 9),
        written(
            "infra/cdk/gate.exceptions.toml",
            "demo/rule#part",
            "D-ONE-001",
            1,
        ),
        written(
            "infra/cdk/gate.exceptions.toml",
            "demo/rule",
            "D-API-002",
            7,
        ),
        written(
            "infra/cdk/gate.exceptions.toml",
            "demo/rule",
            "D-NOPE-004",
            13,
        ),
    ];
    let found = reported_with(decisions, "", "", &files);
    assert_eq!(
        found
            .iter()
            .map(|(path, line, message, _)| (path.as_str(), *line, message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                "infra/cdk/gate.toml",
                Some(7),
                "the entry for \"demo/*\" names the decision D-NOPE-003, which docs/ledger/decision-index.md does not list; name an accepted decision of the index"
            ),
            (
                "infra/cdk/gate.toml",
                Some(9),
                "the entry for \"demo/other\" names the decision D-TWO-001, which is `proposed`; an entry needs an accepted decision"
            ),
            (
                "infra/cdk/gate.exceptions.toml",
                Some(7),
                "the entry for \"demo/rule\" names the decision D-API-002, which is `proposed`; an entry needs an accepted decision"
            ),
            (
                "infra/cdk/gate.exceptions.toml",
                Some(13),
                "the entry for \"demo/rule\" names the decision D-NOPE-004, which docs/ledger/decision-index.md does not list; name an accepted decision of the index"
            ),
        ]
    );
}

#[test]
fn the_call_is_judged_alike_whether_or_not_the_root_is_a_project() {
    // The repository call's own file differs between the two shapes; the judgement does not.
    let files = tree();
    let decisions = vec![
        decision("demo/rule", "D-TWO-001", 4),
        written("apps/one/gate.toml", "demo/*", "D-NOPE-001", 5),
    ];
    let at_root = reported_with(decisions.clone(), "", "gate.toml", &files);
    assert_eq!(at_root.len(), 2, "{at_root:?}");
    assert_eq!(reported_with(decisions, "", "", &files), at_root);
}

#[test]
fn without_an_index_every_decision_is_unlisted() {
    let files = ledger();
    let found = reported(vec![decision("demo/rule", "D-ONE-001", 2)], "", &files);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].3.as_deref(), Some("demo/rule/D-ONE-001#unlisted"));
    assert_eq!(
        found[0].2,
        format!(
            "the entry for \"demo/rule\" names the decision D-ONE-001, which {INDEX} does not list; name an accepted decision of the index"
        )
    );
}

#[test]
fn the_index_is_the_shared_decisions_option() {
    let files = ledger().with(
        "ledger/index.md",
        "| `D-ONE-001` | `accepted` | One | `one.md` |\n",
    );
    let decisions = vec![decision("demo/rule", "D-ONE-001", 2)];
    assert_eq!(
        reported(
            decisions.clone(),
            r#"{"decisions":"ledger/index.md"}"#,
            &files
        ),
        []
    );
    assert_eq!(reported(decisions, "", &files).len(), 1);
}

#[test]
fn the_examples_of_the_rule_page() {
    // plugins/ledger/rules/config-decisions.md: in a root project, `"demo/rule" = { level = "off",
    // decision = "D-API-002" }` on line 2 of infra/cdk/gate.toml, and an exception naming
    // D-NOPE-009 at line 1 of infra/cdk/gate.exceptions.toml, with D-API-002 `proposed`.
    let found = reported_with(
        vec![
            written("infra/cdk/gate.toml", "demo/rule", "D-API-002", 2),
            written(
                "infra/cdk/gate.exceptions.toml",
                "demo/rule",
                "D-NOPE-009",
                1,
            ),
        ],
        "",
        "",
        &tree(),
    );
    assert_eq!(
        found
            .iter()
            .map(|(path, line, message, _)| (path.as_str(), *line, message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                "infra/cdk/gate.toml",
                Some(2),
                "the entry for \"demo/rule\" names the decision D-API-002, which is `proposed`; an entry needs an accepted decision"
            ),
            (
                "infra/cdk/gate.exceptions.toml",
                Some(1),
                "the entry for \"demo/rule\" names the decision D-NOPE-009, which docs/ledger/decision-index.md does not list; name an accepted decision of the index"
            ),
        ]
    );
}

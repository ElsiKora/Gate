//! `ledger/rule-decisions` and `ledger/root-rule-decisions`: the decisions come from the call,
//! never from `gate.toml`.

use gate_sdk::{CheckRequest, FileLocation, Finding, Location, RuleDecision, RuleRun, Span};

use crate::{Files, ledger};

const INDEX: &str = "docs/ledger/decision-index.md";

fn tree() -> Files {
    ledger().with(
        INDEX,
        "| `D-ONE-001` | `accepted` | One | `one.md` |\n| `D-TWO-001` | `proposed` | Two | `two.md` |\n| `D-API-002` | `proposed` | Api | `api.md` |\n",
    )
}

fn decision(key: &str, id: &str, line: u32) -> RuleDecision {
    RuleDecision {
        key: key.to_owned(),
        decision: id.to_owned(),
        at: FileLocation {
            path: "gate.toml".to_owned(),
            span: Some(Span::line(line)),
        },
    }
}

/// Path, line, message and key of each finding of the rule, in a call carrying `decisions`.
fn reported(
    decisions: Vec<RuleDecision>,
    plugin_options: &str,
    files: &Files,
) -> Vec<(String, Option<u32>, String, Option<String>)> {
    reported_by("rule-decisions", decisions, plugin_options, files)
}

/// Path, line, message and key of each finding of `rule`, in a call carrying `decisions`.
fn reported_by(
    rule: &str,
    decisions: Vec<RuleDecision>,
    plugin_options: &str,
    files: &Files,
) -> Vec<(String, Option<u32>, String, Option<String>)> {
    let request = CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: rule.to_owned(),
            options: String::new(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: plugin_options.to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: decisions,
        config_file: "gate.toml".to_owned(),
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
                "the rule entry \"demo/rule\" names the decision D-TWO-001, which is `proposed`; a rule entry needs an accepted decision".to_owned(),
                Some("demo/rule/D-TWO-001#not-accepted".to_owned()),
            ),
            (
                "gate.toml".to_owned(),
                Some(7),
                "the rule entry \"demo/*\" names the decision D-NOPE-001, which docs/ledger/decision-index.md does not list; name an accepted decision of the index".to_owned(),
                Some("demo/*/D-NOPE-001#unlisted".to_owned()),
            ),
        ]
    );
}

#[test]
fn without_an_index_every_decision_is_unlisted() {
    let files = ledger();
    let found = reported(vec![decision("demo/rule", "D-ONE-001", 2)], "", &files);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].3.as_deref(), Some("demo/rule/D-ONE-001#unlisted"));
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
fn the_example_of_the_rule_page() {
    // plugins/ledger/rules/rule-decisions.md: `"demo/rule" = { level = "off", decision =
    // "D-API-002" }` on line 2, with D-API-002 `proposed`.
    let found = reported(vec![decision("demo/rule", "D-API-002", 2)], "", &tree());
    assert_eq!(
        found
            .iter()
            .map(|(path, line, message, _)| (path.as_str(), *line, message.as_str()))
            .collect::<Vec<_>>(),
        [(
            "gate.toml",
            Some(2),
            "the rule entry \"demo/rule\" names the decision D-API-002, which is `proposed`; a rule entry needs an accepted decision"
        )]
    );
}

#[test]
fn the_root_rule_judges_the_decisions_of_its_call_as_the_project_rule_does() {
    // The host gives the repository call the root file's decisions when the root is not a
    // project, and none under a root project; the root rule judges what its call carries.
    let files = tree();
    let decisions = vec![
        decision("demo/rule", "D-TWO-001", 4),
        decision("demo/*", "D-NOPE-001", 7),
        decision("demo/other", "D-ONE-001", 9),
    ];
    let expected = reported(decisions.clone(), "", &files);
    assert_eq!(expected.len(), 2, "{expected:?}");
    assert_eq!(
        reported_by("root-rule-decisions", decisions, "", &files),
        expected
    );
    assert_eq!(
        reported_by("root-rule-decisions", Vec::new(), "", &ledger()),
        []
    );
}

#[test]
fn the_root_rule_without_a_ledger_names_the_index_it_read() {
    // The repository call carries a root decision, and the index is not tracked: the decision is
    // unlisted, and the finding names the index the rule read, as the project rule's does.
    let decisions = vec![decision("demo/rule", "D-ONE-001", 4)];
    let found = reported_by(
        "root-rule-decisions",
        decisions.clone(),
        "",
        &Files::default(),
    );
    assert_eq!(found, reported(decisions, "", &Files::default()));
    assert_eq!(found.len(), 1, "{found:?}");
    let (path, line, message, _) = &found[0];
    assert_eq!((path.as_str(), *line), ("gate.toml", Some(4)));
    assert_eq!(
        message,
        &format!(
            "the rule entry \"demo/rule\" names the decision D-ONE-001, which {INDEX} does not list; name an accepted decision of the index"
        )
    );
}

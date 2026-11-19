//! The ci plugin's rules run natively through the SDK against synthetic trees.

mod actions_pinned;
mod fixture;
mod workflow_run_target;

use fixture::Fixture;

#[test]
fn every_rule_reads_the_repository_and_refuses_unknown_options() {
    let info = gate_sdk::describe(&ci::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["actions-pinned", "workflow-run-target"]);
    for rule in &info.rules {
        assert_eq!(rule.scope, gate_sdk::Scope::Repository);
        let schema: serde_json::Value =
            serde_json::from_str(&rule.options_schema).expect("the schema is JSON");
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
        let error = Fixture::new(&[])
            .check(&rule.name, r#"{"workflow": "x"}"#)
            .expect_err("unknown key");
        assert!(
            error.message().contains("unknown field `workflow`"),
            "{error}"
        );
    }
}

#[test]
fn a_workflow_that_is_not_yaml_or_a_bad_glob_stops_the_run() {
    let broken = Fixture::new(&[(".github/workflows/ci.yml", "jobs: [\n")]);
    let error = broken
        .check("actions-pinned", "{}")
        .expect_err("invalid YAML");
    assert!(
        error
            .message()
            .contains(".github/workflows/ci.yml: while parsing a node"),
        "{error}"
    );

    let error = Fixture::new(&[])
        .check("workflow-run-target", r#"{"workflows": "a/{b"}"#)
        .expect_err("invalid glob");
    assert!(
        error.message().contains("workflows is not a valid glob"),
        "{error}"
    );
}

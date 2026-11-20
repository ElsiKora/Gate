//! The node plugin's rules run natively through the SDK against synthetic trees.

mod dependency_versions;
mod fixture;
mod formatter;
mod linter;
mod node_version;
mod package;
mod package_manager;
mod scripts;
mod tsconfig;
mod tsconfig_base;

use gate_sdk::Scope;

use crate::fixture::Fixture;

#[test]
fn nine_rules_with_options_that_refuse_unknown_keys_and_no_history() {
    let info = gate_sdk::describe(&node::PLUGIN);
    let rules: Vec<(&str, Scope)> = info
        .rules
        .iter()
        .map(|rule| (rule.name.as_str(), rule.scope))
        .collect();
    assert_eq!(
        rules,
        [
            ("package-manager", Scope::Repository),
            ("node-version", Scope::Repository),
            ("dependency-versions", Scope::Repository),
            ("formatter", Scope::Repository),
            ("linter", Scope::Repository),
            ("package", Scope::Project),
            ("scripts", Scope::Project),
            ("tsconfig", Scope::Project),
            ("tsconfig-base", Scope::Repository),
        ]
    );
    for rule in &info.rules {
        let schema: serde_json::Value =
            serde_json::from_str(&rule.options_schema).expect("the schema is JSON");
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false),
            "{}",
            rule.name
        );
        assert!(!rule.reads_history, "{}", rule.name);
    }
    let options: serde_json::Value =
        serde_json::from_str(&info.options_schema).expect("the schema is JSON");
    assert_eq!(options["required"], serde_json::json!(["kind"]));
    assert_eq!(
        options["additionalProperties"],
        serde_json::Value::Bool(false)
    );
}

/// A missing or unknown kind, and a separate root without its job or with a path that climbs, are
/// configuration errors that name the key.
#[test]
fn the_plugin_options_are_checked_before_any_rule_runs() {
    let tree = [("package.json", "{}")];
    let cases = [
        ("{}", "missing field `kind`"),
        (r#"{"kind": "backend"}"#, "unknown variant `backend`"),
        (
            r#"{"kind": "service", "separate": [{"path": "spikes/x", "job": " "}]}"#,
            "separate[0].job is empty",
        ),
        (
            r#"{"kind": "service", "separate": [{"path": "../x", "job": "a spike"}]}"#,
            "separate[0].path is \"../x\"",
        ),
        (
            r#"{"kind": "service", "separate": [{"path": "spikes/x/", "job": "a spike"}]}"#,
            "separate[0].path",
        ),
        (
            r#"{"kind": "service", "base": "../tsconfig.base.json"}"#,
            "options base is \"../tsconfig.base.json\"",
        ),
        (
            r#"{"kind": "service", "other": 1}"#,
            "unknown field `other`",
        ),
    ];
    for (options, expected) in cases {
        let error = Fixture::new("service", &tree)
            .plugin_options(options)
            .check("package", "{}")
            .expect_err(options);
        assert!(error.is_config(), "{options}");
        assert!(
            error.message().contains(expected),
            "{options}: {}",
            error.message()
        );
    }
    let listed = Fixture::new("service", &tree)
        .plugin_options(r#"{"kind": "backend"}"#)
        .check("package", "{}")
        .expect_err("an unknown kind");
    assert!(
        listed
            .message()
            .contains("service, frontend, infrastructure, library, monorepo-root")
    );
}

/// A file that is not JSON fails the call, naming the file and the line.
#[test]
fn a_manifest_that_is_not_json_fails_the_call_and_names_it() {
    let error = Fixture::new("service", &[("package.json", "{\n  \"name\": ,\n}\n")])
        .check("package", "{}")
        .expect_err("broken JSON");
    assert!(!error.is_config());
    assert!(
        error
            .message()
            .ends_with("package.json is not JSON (line 2: a value is expected); fix it so the node rules can read it"),
        "{}",
        error.message()
    );
}

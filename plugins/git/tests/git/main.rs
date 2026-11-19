//! The git plugin's rules run natively through the SDK against synthetic histories.

mod branch_name;
mod commit_message;
mod main_history;
mod no_ci_skip;
mod repo;

use repo::{Repo, settings};

#[test]
fn the_rules_read_history_over_the_repository() {
    let info = gate_sdk::describe(&git::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "main-history",
            "no-ci-skip",
            "branch-name",
            "commit-message"
        ]
    );
    for rule in &info.rules {
        assert!(rule.reads_history, "{}", rule.name);
        assert_eq!(rule.scope, gate_sdk::Scope::Repository);
        let schema: serde_json::Value =
            serde_json::from_str(&rule.options_schema).expect("the schema is JSON");
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false)
        );
    }
    assert_eq!(info.rules[0].aspects, ["direct", "merge", "release"]);
}

#[test]
fn the_recommended_preset_keeps_the_references_warnings_as_warnings() {
    let info = gate_sdk::describe(&git::PLUGIN);
    let preset = info
        .presets
        .iter()
        .find(|preset| preset.name == "recommended")
        .expect("git/recommended");
    let rules: serde_json::Value = serde_json::from_str(&preset.rules).expect("JSON");
    assert_eq!(
        rules["commit-message"]["levels"],
        serde_json::json!({ "header-min-length": "warning", "scope-max-length": "warning" })
    );
    assert_eq!(
        rules["branch-name"]["options"]["template"],
        ":type/:ticket-:name"
    );
    // Every aspect the preset sets is one its rule declares.
    for (rule, settings) in rules.as_object().expect("rules") {
        let declared = &info
            .rules
            .iter()
            .find(|info| info.name == *rule)
            .expect("a rule of the plugin")
            .aspects;
        for aspect in settings["levels"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, _)| key)
        {
            assert!(declared.contains(aspect), "{rule}#{aspect}");
        }
    }
}

#[test]
fn the_plugin_options_schema_requires_main_and_types_the_forms() {
    let info = gate_sdk::describe(&git::PLUGIN);
    let schema: serde_json::Value =
        serde_json::from_str(&info.options_schema).expect("the schema is JSON");
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .expect("required keys")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["main"]);
    assert_eq!(
        schema["additionalProperties"],
        serde_json::Value::Bool(false)
    );
    assert!(
        schema["properties"]["branch-length"].is_object(),
        "{schema}"
    );
    assert!(schema["properties"]["forms"].is_object(), "{schema}");
}

/// The keys of `options` a host reports missing for rule `rule`: those it declares it reads, absent
/// from `options`, whose condition `options` match.
fn missing(rule: &str, options: &serde_json::Value) -> Vec<String> {
    let info = gate_sdk::describe(&git::PLUGIN);
    let rule = info
        .rules
        .iter()
        .find(|info| info.name == rule)
        .expect("declared");
    let schema: serde_json::Value =
        serde_json::from_str(&rule.options_schema).expect("the schema is JSON");
    let Some(needs) = schema.get(gate_sdk::NEEDS_KEYWORD) else {
        return Vec::new();
    };
    needs
        .as_array()
        .expect("a list")
        .iter()
        .filter(|need| {
            let key = need["key"].as_str().expect("a key");
            assert!(need["why"].is_string(), "{need}");
            options.get(key).is_none()
                && (need["when"].is_null()
                    || jsonschema::validator_for(&need["when"])
                        .expect("a JSON Schema")
                        .is_valid(options))
        })
        .map(|need| need["key"].as_str().expect("a key").to_owned())
        .collect()
}

#[test]
fn the_history_rules_declare_the_options_they_read_by_their_forms() {
    let main = serde_json::json!({ "main": ["refs/heads/main"] });
    let with = |forms: serde_json::Value| {
        let mut options = main.clone();
        options["forms"] = forms;
        options
    };
    let merge = |checks: serde_json::Value| serde_json::json!({ "kind": "merge", "parents": 2, "subject": "^Merge branch '([^']+)'$", "checks": checks });
    let direct = serde_json::json!({ "kind": "direct", "parents": 1 });
    let release = serde_json::json!({ "kind": "release", "parents": 1 });

    // The default forms read every key.
    assert_eq!(
        missing("main-history", &main),
        [
            "activation",
            "release-subject",
            "branch-pattern",
            "branch-length",
            "dependabot",
            "documentation"
        ]
    );
    assert_eq!(
        missing("no-ci-skip", &main),
        ["activation", "release-subject"]
    );
    // Forms read the keys of their kinds and checks.
    assert_eq!(
        missing(
            "main-history",
            &with(serde_json::json!([merge(serde_json::json!(["body"]))]))
        ),
        ["activation"]
    );
    assert_eq!(
        missing(
            "main-history",
            &with(serde_json::json!([merge(serde_json::json!([
                "body", "branch"
            ]))]))
        ),
        [
            "activation",
            "branch-pattern",
            "branch-length",
            "dependabot"
        ]
    );
    assert_eq!(
        missing(
            "main-history",
            &with(
                serde_json::json!([{ "kind": "merge", "parents": 2, "subject": "^Merge branch '([^']+)'$" }])
            )
        ),
        [
            "activation",
            "branch-pattern",
            "branch-length",
            "dependabot"
        ]
    );
    assert_eq!(
        missing("main-history", &with(serde_json::json!([direct, release]))),
        ["activation", "release-subject", "documentation"]
    );
    assert_eq!(
        missing(
            "main-history",
            &with(serde_json::json!([{ "kind": "direct", "parents": 1, "checks": [] }]))
        ),
        ["activation"]
    );
    assert_eq!(
        missing("no-ci-skip", &with(serde_json::json!([direct]))),
        ["activation"]
    );
    // A key that is there is not missing.
    let mut set = main.clone();
    set["activation"] = serde_json::json!("a".repeat(40));
    set["release-subject"] = serde_json::json!("^v(.+)$");
    assert_eq!(missing("no-ci-skip", &set), Vec::<String>::new());
    // The rules of the branch and the message read none.
    assert_eq!(missing("branch-name", &main), Vec::<String>::new());
    assert_eq!(missing("commit-message", &main), Vec::<String>::new());
}

#[test]
fn the_settings_are_required_and_checked() {
    let mut repo = Repo::default();
    let root = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &root);

    for rule in ["main-history", "no-ci-skip", "branch-name"] {
        let options = if rule == "branch-name" {
            r#"{"pattern": "^feat/"}"#
        } else {
            "{}"
        };
        let missing = repo.check(rule, "{}", options).expect_err("no settings");
        assert!(
            missing.message().contains("missing field `main`"),
            "{missing}"
        );
        assert!(missing.is_config(), "a configuration error: {missing}");
        if rule == "branch-name" {
            continue;
        }

        let short = settings("abc");
        let error = repo
            .check(rule, &short, "{}")
            .expect_err("short activation");
        assert!(
            error.message().contains("is not a full commit id"),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");

        let unknown = settings(&root).replace("\"main\"", "\"mainline\"");
        let error = repo.check(rule, &unknown, "{}").expect_err("unknown key");
        assert!(
            error.message().contains("unknown field `mainline`"),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");

        let generated =
            settings(&root).replace("\"main\"", "\"generated\": [\".agents/**\"], \"main\"");
        let error = repo
            .check(rule, &generated, "{}")
            .expect_err("generated is not an option");
        assert!(
            error.message().contains("unknown field `generated`"),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");

        let elsewhere = "b".repeat(40);
        let error = repo
            .check(rule, &settings(&elsewhere), "{}")
            .expect_err("not on main");
        assert!(
            error.message().contains("is not an ancestor of main"),
            "{error}"
        );
        assert!(!error.is_config(), "an environment error: {error}");

        let groupless = settings(&root).replace(r"(\\d+\\.\\d+\\.\\d+)", r"\\d+\\.\\d+\\.\\d+");
        assert_ne!(groupless, settings(&root));
        let error = repo
            .check(rule, &groupless, "{}")
            .expect_err("no capture group");
        assert!(error.message().contains("has no capture group"), "{error}");
        assert!(error.is_config(), "a configuration error: {error}");

        // Read as JavaScript reads it: an inline flag is not JavaScript.
        let flag = settings(&root).replace(r"(\\d+\\.\\d+\\.\\d+)", r"(?i)(\\d+\\.\\d+\\.\\d+)");
        assert_ne!(flag, settings(&root));
        let error = repo.check(rule, &flag, "{}").expect_err("an inline flag");
        assert!(
            error
                .message()
                .contains("has a group (? other than (?: and (?<name>"),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");

        let inverted = settings(&root).replace("[8, 30]", "[30, 8]");
        let error = repo
            .check(rule, &inverted, "{}")
            .expect_err("min above max");
        assert!(
            error
                .message()
                .contains("has its minimum above its maximum"),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");

        let nowhere = settings(&root).replace("refs/heads/main", "refs/heads/trunk");
        let error = repo.check(rule, &nowhere, "{}").expect_err("no main");
        assert!(
            error.message().contains("none of the main references"),
            "{error}"
        );
        assert!(!error.is_config(), "an environment error: {error}");
    }

    let partial = format!(
        r#"{{"main": ["refs/heads/main"], "activation": "{root}", "release-subject": "^(r)$"}}"#
    );
    assert_eq!(repo.check("no-ci-skip", &partial, "{}").expect("runs"), []);
    let error = repo
        .check("main-history", &partial, "{}")
        .expect_err("incomplete");
    assert!(
        error.message().contains(
            "has no branch-pattern; the branch check of [plugins.git] options form 1 (merge) reads it"
        ),
        "{error}"
    );
    assert!(error.is_config(), "a configuration error: {error}");

    // Forms that read nothing else: only `activation` is missing.
    let main_only = r#"{"main": ["refs/heads/main"], "forms": [{"kind": "direct", "checks": []}]}"#;
    for rule in ["main-history", "no-ci-skip"] {
        let error = repo
            .check(rule, main_only, "{}")
            .expect_err("no activation");
        assert!(
            error.message().contains(&format!(
                "has no activation; git/{rule} reads history after it"
            )),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");
    }

    // The default forms have a release form, which reads `release-subject`.
    let unreleased = format!(r#"{{"main": ["refs/heads/main"], "activation": "{root}"}}"#);
    for rule in ["main-history", "no-ci-skip"] {
        let error = repo
            .check(rule, &unreleased, "{}")
            .expect_err("no release-subject");
        assert!(
            error.message().contains(
                "has no release-subject; [plugins.git] options form 2 (release) matches the release commits by it"
            ),
            "{error}"
        );
        assert!(error.is_config(), "a configuration error: {error}");
    }
}

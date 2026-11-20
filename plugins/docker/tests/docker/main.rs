//! The docker plugin's rules run natively through the SDK against synthetic trees.

mod compose;
mod dockerfile;
mod fixture;

use std::collections::BTreeMap;

use gate_sdk::Scope;
use serde_json::Value;

#[test]
fn both_rules_read_their_project_and_refuse_unknown_options() {
    let info = gate_sdk::describe(&docker::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["dockerfile", "compose"]);
    for rule in &info.rules {
        assert_eq!(rule.scope, Scope::Project);
        let error = fixture::Fixture::new(&[])
            .check_with(&rule.name, r#"{"unknown": 1}"#, "")
            .expect_err("unknown key");
        assert!(
            error.message().contains("unknown field `unknown`"),
            "{error}"
        );
    }
    let error = fixture::Fixture::new(&[])
        .check_with("dockerfile", "{}", r#"{"secret-name": []}"#)
        .expect_err("unknown plugin key");
    assert!(error.message().contains("[plugins.docker]"), "{error}");
}

/// For each preset of `plugin`: its rules with their level, and the aspects it sets with theirs.
fn presets(plugin: &gate_sdk::PluginInfo) -> BTreeMap<String, Vec<(String, String)>> {
    plugin
        .presets
        .iter()
        .map(|preset| {
            let rules: Value = serde_json::from_str(&preset.rules).expect("the preset is JSON");
            let mut levels = Vec::new();
            for (rule, settings) in rules.as_object().expect("rules by name") {
                let declared = plugin
                    .rules
                    .iter()
                    .find(|info| info.name == *rule)
                    .unwrap_or_else(|| panic!("{} names the unknown rule {rule}", preset.name));
                levels.push((
                    rule.clone(),
                    settings["level"].as_str().unwrap_or("").to_owned(),
                ));
                for (aspect, level) in settings["levels"].as_object().into_iter().flatten() {
                    assert!(
                        declared.aspects.contains(aspect),
                        "{} sets {rule}#{aspect}, which the rule does not declare",
                        preset.name
                    );
                    levels.push((
                        format!("{rule}#{aspect}"),
                        level.as_str().unwrap_or("").to_owned(),
                    ));
                }
            }
            (preset.name.clone(), levels)
        })
        .collect()
}

#[test]
fn each_preset_enables_exactly_the_plugins_rules_at_its_level() {
    let info = gate_sdk::describe(&docker::PLUGIN);
    let at = |level: &str| -> Vec<(String, String)> {
        info.rules
            .iter()
            .map(|rule| (rule.name.clone(), level.to_owned()))
            .collect()
    };
    let presets = presets(&info);
    assert_eq!(
        presets.keys().collect::<Vec<_>>(),
        ["adoption", "recommended"]
    );
    assert_eq!(presets["recommended"], at("error"));
    assert_eq!(presets["adoption"], at("warning"));
}

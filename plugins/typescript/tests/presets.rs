//! Both presets of the `typescript` plugin turn on the same rules: every rule but the ones left
//! out, each at the preset's one level. A new rule fails here until it joins the presets or this
//! list.

use gate_sdk::serde_json::{self, json};

/// The rules no preset turns on, each with the reason the plugins guide gives.
const LEFT_OUT: &[(&str, &str)] = &[];

#[test]
fn both_presets_turn_on_every_rule_but_the_ones_left_out() {
    let info = gate_sdk::describe(&typescript::PLUGIN);
    for (rule, _) in LEFT_OUT {
        assert!(
            info.rules.iter().any(|known| known.name == *rule),
            "{rule} is no rule of the plugin"
        );
    }
    let mut expected: Vec<&str> = info
        .rules
        .iter()
        .map(|rule| rule.name.as_str())
        .filter(|name| !LEFT_OUT.iter().any(|(rule, _)| rule == name))
        .collect();
    expected.sort_unstable();
    let presets: Vec<&str> = info
        .presets
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    assert_eq!(presets, ["recommended", "adoption"]);
    for (preset, level) in [("recommended", "error"), ("adoption", "warning")] {
        let text = &info
            .presets
            .iter()
            .find(|candidate| candidate.name == preset)
            .expect("the preset")
            .rules;
        let rules: serde_json::Value = serde_json::from_str(text).expect("the preset is JSON");
        let rules = rules.as_object().expect("an object");
        let mut names: Vec<&str> = rules.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, expected, "{preset}");
        for (rule, settings) in rules {
            assert_eq!(settings, &json!({ "level": level }), "{preset} {rule}");
        }
    }
}

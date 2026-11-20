//! The docs plugin runs natively through the SDK against synthetic trees: the standards' tables,
//! the generator's seeds per kind, and the rule's aspects, each with a passing twin.

mod fixture;
mod lay_down;
mod layout;
mod presets;
mod standard;

/// Each preset turns on exactly the plugin's rules, every one at the preset's level and with
/// nothing else: `docs/recommended` at error, `docs/adoption` at warning.
#[test]
fn each_preset_enables_exactly_the_plugins_rules_at_its_level() {
    let info = gate_sdk::describe(&docs::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["layout"]);
    let declared: Vec<&str> = info
        .presets
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    assert_eq!(declared, ["recommended", "adoption"]);
    for (preset, level) in [("recommended", "error"), ("adoption", "warning")] {
        let rules = info
            .presets
            .iter()
            .find(|candidate| candidate.name == preset)
            .map(|found| serde_json::from_str::<serde_json::Value>(&found.rules).expect("JSON"))
            .expect("declared");
        let expected: serde_json::Map<String, serde_json::Value> = names
            .iter()
            .map(|name| ((*name).to_owned(), serde_json::json!({ "level": level })))
            .collect();
        assert_eq!(rules, serde_json::Value::Object(expected), "docs/{preset}");
    }
}

//! Every rule a preset of the `cdk` plugin turns on loads with what the preset gives it: its
//! options schema requires no key the preset leaves out, and it needs no plugin option whatever its
//! options are. A rule whose option becomes required fails here before a project that extends the
//! preset does.

use gate_sdk::serde_json;

#[test]
fn every_rule_a_preset_turns_on_loads_with_what_the_preset_gives() {
    let info = gate_sdk::describe(&cdk::PLUGIN);
    for preset in &info.presets {
        let rules: serde_json::Value =
            serde_json::from_str(&preset.rules).expect("the preset is JSON");
        for (rule, settings) in rules.as_object().expect("an object") {
            assert_loads(&info, &preset.name, rule, settings);
        }
    }
}

/// The rule loads with what the preset gives it: every option its schema requires (or every one of
/// a `oneOf` alternative), and no plugin option it needs whatever the options are.
fn assert_loads(
    info: &gate_sdk::PluginInfo,
    preset: &str,
    rule: &str,
    settings: &serde_json::Value,
) {
    let text = &info
        .rules
        .iter()
        .find(|known| known.name == rule)
        .expect("a rule of the plugin")
        .options_schema;
    let schema: serde_json::Value = serde_json::from_str(text).expect("the schema is JSON");
    let given = settings
        .get("options")
        .and_then(serde_json::Value::as_object);
    assert!(
        holds(given, &schema["required"]),
        "{preset} {rule} leaves out a required option: {}",
        schema["required"]
    );
    if let Some(alternatives) = schema["oneOf"].as_array() {
        assert!(
            alternatives
                .iter()
                .any(|alternative| holds(given, &alternative["required"])),
            "{preset} {rule} gives no alternative of its options"
        );
    }
    for need in schema[gate_sdk::NEEDS_KEYWORD]
        .as_array()
        .into_iter()
        .flatten()
    {
        assert!(
            !need["when"].is_null(),
            "{preset} {rule} needs the plugin option {} whatever the options are",
            need["key"]
        );
    }
}

/// Whether `given` holds every key of `required` (none when `required` is absent).
fn holds(
    given: Option<&serde_json::Map<String, serde_json::Value>>,
    required: &serde_json::Value,
) -> bool {
    required.as_array().is_none_or(|keys| {
        keys.iter().all(|key| {
            key.as_str()
                .is_some_and(|key| given.is_some_and(|options| options.contains_key(key)))
        })
    })
}

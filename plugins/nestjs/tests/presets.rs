//! Both presets of the `nestjs` plugin turn on the same rules: every rule but the ones left out,
//! each at the preset's one level. A new rule fails here until it joins the presets or this list.

use gate_sdk::serde_json::{self, json};

/// The rules no preset turns on, each with the reason the plugins guide gives.
const LEFT_OUT: &[(&str, &str)] = &[
    ("advisory-lock-boundary", "`executor` has no default"),
    (
        "advisory-lock-manifest",
        "only a service with advisory locks has its manifest",
    ),
    (
        "asset-storage",
        "only a service that stores assets has its adapter",
    ),
    (
        "controller-owners",
        "the `controllers` registry is the project's own",
    ),
    (
        "field-inventory",
        "the `fields` inventory is the project's own",
    ),
    ("import-direction", "`contracts` has no default"),
    (
        "kind-imports",
        "a service takes on the kind-barrel import form by choice",
    ),
    (
        "notification-egress",
        "`owner-classes`, `egress.modules` and `owners` have no default",
    ),
    (
        "owner-boundaries",
        "the owner roots, owners and routes have no default",
    ),
    ("owner-roots", "`required-roots` has no default"),
    ("package-pins", "the `artifacts` list is the project's own"),
    ("raw-config-access", "`raw-imports` has no default"),
    ("runtime-factory", "`contracts` has no default"),
];

#[test]
fn both_presets_turn_on_every_rule_but_the_ones_left_out() {
    let info = gate_sdk::describe(&nestjs::PLUGIN);
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
            assert_loads(&info, preset, rule, settings);
        }
    }
}

/// The rule loads with what the preset gives it: every option its schema requires (or every one of
/// a `oneOf` alternative), and no plugin option it needs whatever the options are. A rule whose
/// option becomes required fails here before a project that extends the preset does.
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

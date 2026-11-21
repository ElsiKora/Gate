//! Both presets of the `frontend` plugin turn on the same rules: every rule but the ones left
//! out, each at the preset's one level. A new rule fails here until it joins the presets or this
//! list.
//!
//! Every rule of the plugin needs `pack`, the plugin's one required setting: which framework the
//! project is. No preset can choose it, so a project that extends a preset sets
//! `[plugins.frontend] options pack` first. The load check gives each preset rule each pack in turn
//! and checks every other need and every required option as the other plugins' checks do.

mod support;

use gate_sdk::serde_json::{self, json};

/// The values of `pack`, the one plugin option a preset rule may need whatever its options are.
const PACKS: [&str; 3] = ["react-vite", "nextjs", "react-library"];

/// The error a project that extends a preset without a pack gets: it names `pack`, its three
/// values and the table to write it in.
const NO_PACK: &str = "[plugins.frontend] options has no pack; the standard differs by framework \
    where the framework requires it: write pack = \"react-vite\", \"nextjs\" or \"react-library\"";

/// The rules no preset turns on, each with the reason the plugins guide gives.
const LEFT_OUT: &[(&str, &str)] = &[];

#[test]
fn both_presets_turn_on_every_rule_but_the_ones_left_out() {
    let info = gate_sdk::describe(&frontend::PLUGIN);
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
            for pack in PACKS {
                assert_loads(&info, preset, rule, settings, pack);
            }
            let error = support::run_raw(rule, "{}", "{}", &[])
                .expect_err("a preset rule without a pack does not run");
            assert!(
                error.message().contains(NO_PACK),
                "{preset} {rule}: {}",
                error.message()
            );
        }
    }
}

/// The rule loads with what the preset gives it and the pack `pack`: every option its schema
/// requires (or every one of a `oneOf` alternative), no plugin option but `pack` it needs whatever
/// the options are, and it runs. A rule whose option becomes required fails here before a project
/// that extends the preset does.
fn assert_loads(
    info: &gate_sdk::PluginInfo,
    preset: &str,
    rule: &str,
    settings: &serde_json::Value,
    pack: &str,
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
            !need["when"].is_null() || need["key"] == "pack",
            "{preset} {rule} needs the plugin option {} whatever the options are",
            need["key"]
        );
    }
    let options = settings
        .get("options")
        .map_or_else(|| "{}".to_owned(), serde_json::Value::to_string);
    let plugin_options = json!({ "pack": pack }).to_string();
    if let Err(error) = support::run_raw(rule, &options, &plugin_options, &[]) {
        panic!("{preset} {rule} with pack {pack}: {}", error.message());
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

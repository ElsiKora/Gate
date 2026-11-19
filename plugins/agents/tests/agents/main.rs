//! The agents plugin's rules and generators run natively through the SDK against synthetic
//! trees.

mod fixture;
mod generators;
mod instruction_references;
mod rules;
mod skills;

#[test]
fn every_rule_and_generator_declares_options_that_refuse_unknown_keys() {
    let info = gate_sdk::describe(&agents::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "instruction-references",
            "no-claude-md",
            "rule-enforcement",
            "rule-file-size",
            "rule-ids",
            "rule-owner",
            "rule-references",
            "rule-registry",
            "skills",
        ]
    );
    let generators: Vec<&str> = info
        .generators
        .iter()
        .map(|generator| generator.name.as_str())
        .collect();
    assert_eq!(generators, ["adapters", "agents-block"]);
    let schemas = info
        .rules
        .iter()
        .map(|rule| (&rule.name, &rule.options_schema))
        .chain(
            info.generators
                .iter()
                .map(|generator| (&generator.name, &generator.options_schema)),
        );
    for (name, schema) in schemas {
        let schema: serde_json::Value = serde_json::from_str(schema).expect("the schema is JSON");
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false),
            "{name}"
        );
    }
    assert!(info.rules.iter().all(|rule| !rule.reads_history));
}

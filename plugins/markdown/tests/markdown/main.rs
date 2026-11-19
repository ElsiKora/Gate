//! The markdown plugin's rules run natively through the SDK against synthetic trees.

mod fixture;
mod frontmatter;
mod local_links;
mod required_sections;

#[test]
fn every_rule_declares_options_that_refuse_unknown_keys() {
    let info = gate_sdk::describe(&markdown::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["frontmatter", "local-links", "required-sections"]);
    for rule in &info.rules {
        let schema: serde_json::Value =
            serde_json::from_str(&rule.options_schema).expect("the schema is JSON");
        assert_eq!(
            schema["additionalProperties"],
            serde_json::Value::Bool(false),
            "{}",
            rule.name
        );
        assert!(!rule.reads_history);
    }
}

//! An app at `infrastructure` whose tsconfig extends the base beside it at the repository root,
//! the layout `node/tsconfig` wants of a project in a monorepo: the base is read through the tree
//! when the plugin is granted it, and is one finding of the call with the grant to add when it is
//! not, never an environment error.

use gate_sdk::{CheckRequest, Finding, Location, RuleRun};

use super::{Files, RULES, app};

const TSCONFIG: &str =
    r#"{ "extends": "../tsconfig.base.json", "include": ["bin", "lib", "test"] }"#;

const BASE: &str = r#"{ "compilerOptions": { "strict": true } }"#;

/// Every rule of the plugin over `files`, as the call of the project at `infrastructure` runs them.
fn run(files: &Files) -> gate_sdk::Result<Vec<Finding>> {
    let request = CheckRequest {
        project: "infrastructure".to_owned(),
        rules: RULES
            .iter()
            .map(|rule| RuleRun {
                name: (*rule).to_owned(),
                options: String::new(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: "{}".to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
    };
    gate_sdk::check(&cdk::PLUGIN, files, &request)
}

#[test]
fn an_app_whose_tsconfig_extends_a_granted_base_above_it_passes_every_rule() {
    let files = app()
        .with("tsconfig.json", TSCONFIG)
        .with("../tsconfig.base.json", BASE);

    let findings: Vec<String> = run(&files)
        .expect("the rules run")
        .into_iter()
        .map(|finding| format!("{}#{} {}", finding.rule, finding.aspect, finding.message))
        .collect();
    assert_eq!(findings, Vec::<String>::new());
}

#[test]
fn a_base_the_plugin_cannot_read_is_one_finding_of_the_call_with_the_grant_to_add() {
    let files = app().with("tsconfig.json", TSCONFIG);

    let findings = run(&files).expect("the rules run without an environment error");
    let found: Vec<_> = findings
        .iter()
        .map(|finding| {
            let path = match &finding.location {
                Some(Location::File(file)) => file.path.clone(),
                _ => String::new(),
            };
            (finding.aspect.as_str(), path, finding.target.as_deref())
        })
        .collect();
    assert_eq!(
        found,
        [(
            "",
            "tsconfig.json".to_owned(),
            Some("tsconfig.json:../tsconfig.base.json")
        )]
    );
    // The message names the base relative to the project root, never by a path of the resolver.
    assert!(
        findings[0].message.contains("\"../tsconfig.base.json\""),
        "{}",
        findings[0].message
    );
    assert!(!findings[0].message.contains("/gate/"));
    let help = findings[0].help.as_deref().unwrap_or_default();
    assert!(help.contains(r#"reads = ["tsconfig.base.json"]"#), "{help}");
}

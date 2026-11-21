use crate::fixture::{Fixture, message};
use crate::tsconfig::BASE;

const RULE: &str = "tsconfig-base";

const EXTENDS: &str = "{ \"extends\": \"./tsconfig.base.json\" }";

fn run(files: &[(&str, &str)]) -> Vec<String> {
    Fixture::new("monorepo-root", files).lines(RULE)
}

#[test]
fn the_standard_base_passes_whether_or_not_a_project_owns_it() {
    let files = [
        ("tsconfig.base.json", BASE),
        (
            "apps/a/tsconfig.json",
            "{ \"extends\": \"../../tsconfig.base.json\" }",
        ),
    ];
    assert_eq!(run(&files), Vec::<String>::new());
}

#[test]
fn a_base_without_a_standard_flag_or_the_common_resolution_is_reported() {
    let lacking = BASE.replace("    \"verbatimModuleSyntax\": true,\n", "");
    assert_eq!(
        run(&[("tsconfig.base.json", &lacking), ("tsconfig.json", EXTENDS)]),
        ["flags tsconfig.base.json compilerOptions"]
    );
    let bundled = BASE.replace("\"module\": \"NodeNext\"", "\"module\": \"ESNext\"");
    assert_eq!(
        run(&[("tsconfig.base.json", &bundled), ("tsconfig.json", EXTENDS)]),
        ["flags tsconfig.base.json:13 module"]
    );
    let fixture = Fixture::new(
        "monorepo-root",
        &[("tsconfig.base.json", &lacking), ("tsconfig.json", EXTENDS)],
    );
    assert_eq!(
        message(&fixture, RULE),
        "the base tsconfig.base.json leaves out the standard setting verbatimModuleSyntax"
    );
}

#[test]
fn the_settings_a_base_leaves_out_are_one_finding_and_a_wrong_value_is_one_at_its_line() {
    let without = |base: &str, flag: &str| {
        let line = base
            .lines()
            .find(|line| line.contains(&format!("\"{flag}\"")))
            .expect("the standard base sets the flag");
        base.replace(&format!("{line}\n"), "")
    };
    let mut lacking = BASE.to_owned();
    for flag in ["strict", "isolatedModules", "noImplicitReturns", "module"] {
        lacking = without(&lacking, flag);
    }
    let lacking = lacking
        .replace(
            "\"moduleResolution\": \"NodeNext\"",
            "\"moduleResolution\": \"Bundler\"",
        )
        .replace(
            "\"exactOptionalPropertyTypes\": true",
            "\"exactOptionalPropertyTypes\": false",
        );
    let files = [
        ("tsconfig.base.json", lacking.as_str()),
        ("tsconfig.json", EXTENDS),
    ];
    let mut found = run(&files);
    found.sort();
    assert_eq!(
        found,
        [
            "flags tsconfig.base.json compilerOptions",
            "flags tsconfig.base.json:10 moduleResolution",
            "flags tsconfig.base.json:4 exactOptionalPropertyTypes",
        ]
    );
    let findings = Fixture::new("monorepo-root", &files)
        .check(RULE, "{}")
        .expect("the rule runs");
    let grouped = findings
        .iter()
        .find(|finding| finding.key.as_deref() == Some("compilerOptions"))
        .expect("one finding at the file");
    assert_eq!(
        grouped.message,
        "the base tsconfig.base.json leaves out 4 standard settings: strict, noImplicitReturns, isolatedModules, module"
    );
    assert_eq!(
        grouped.help.as_deref(),
        Some(
            "add to its compilerOptions: \"strict\": true, \"noImplicitReturns\": true, \"isolatedModules\": true, \"module\": \"NodeNext\"; the base carries every standard flag and the common resolution"
        )
    );
}

#[test]
fn an_install_with_typescript_and_no_base_is_reported_and_one_without_typescript_is_not() {
    assert_eq!(
        run(&[("apps/a/tsconfig.json", "{}")]),
        ["missing tsconfig.base.json tsconfig.base.json"]
    );
    assert_eq!(run(&[("package.json", "{}")]), Vec::<String>::new());
}

#[test]
fn a_separate_install_has_a_base_of_its_own_and_the_base_path_is_an_option() {
    let lacking = BASE.replace("    \"strict\": true,\n", "");
    let files = [
        ("tsconfig.base.json", BASE),
        ("tsconfig.json", EXTENDS),
        ("spikes/load/tsconfig.base.json", lacking.as_str()),
        ("spikes/load/tsconfig.json", EXTENDS),
        ("config/base.json", BASE),
    ];
    let separate = r#"{"kind": "monorepo-root", "separate": [{"path": "spikes/load", "job": "a load test with its own install"}]}"#;
    assert_eq!(
        Fixture::new("monorepo-root", &files)
            .plugin_options(separate)
            .lines(RULE),
        ["flags spikes/load/tsconfig.base.json compilerOptions"]
    );
    let moved = r#"{"kind": "monorepo-root", "base": "config/base.json"}"#;
    assert_eq!(
        Fixture::new("monorepo-root", &files[..2])
            .plugin_options(moved)
            .lines(RULE),
        ["missing config/base.json base.json"]
    );
}

use crate::fixture::{Fixture, lines};

const RULE: &str = "tsconfig";

/// The standard base: every standard flag on and the common resolution.
pub const BASE: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitOverride": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "useUnknownInCatchVariables": true,
    "forceConsistentCasingInFileNames": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true,
    "module": "NodeNext",
    "moduleResolution": "NodeNext"
  }
}
"#;

const EXTENDS_BASE: &str = "{\n  \"extends\": \"../../tsconfig.base.json\"\n}\n";

fn member(kind: &str, project: &str, files: &[(&str, &str)]) -> Vec<String> {
    Fixture::new(kind, files).project(project).lines(RULE)
}

#[test]
fn a_chain_that_reaches_the_base_passes_with_comments_and_trailing_commas() {
    let files = [
        (
            "tsconfig.json",
            "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"compilerOptions\": { \"noEmit\": true }\n}\n",
        ),
        (
            "tsconfig.build.json",
            "{ \"extends\": \"./tsconfig.json\" }",
        ),
        (
            "tsconfig.spec.json",
            "// the tests\n{\n  \"extends\": \"./tsconfig\",\n  /* their types */\n  \"compilerOptions\": { \"types\": [\"node\"], },\n}\n",
        ),
    ];
    assert_eq!(
        member("library", "packages/a", &files),
        Vec::<String>::new()
    );
}

#[test]
fn a_configuration_that_repeats_the_flags_instead_of_extending_is_a_copy() {
    let copy = "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noImplicitReturns\": true,\n    \"noFallthroughCasesInSwitch\": true,\n    \"isolatedModules\": true\n  }\n}\n";
    assert_eq!(
        member("service", "apps/s", &[("tsconfig.json", copy)]),
        [
            "copy tsconfig.json tsconfig.json",
            "base tsconfig.json tsconfig.json"
        ]
    );
    let package = "{\n  \"extends\": \"@tsconfig/node24/tsconfig.json\"\n}\n";
    assert_eq!(
        member("service", "apps/s", &[("tsconfig.json", package)]),
        ["base tsconfig.json:2 tsconfig.json"]
    );
}

#[test]
fn turning_a_standard_flag_off_weakens_the_base() {
    let weak = "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"compilerOptions\": {\n    \"strict\": false,\n    \"strictNullChecks\": false,\n    \"noEmit\": false\n  }\n}\n";
    assert_eq!(
        member("service", "apps/s", &[("tsconfig.json", weak)]),
        [
            "weakened tsconfig.json:4 strict",
            "weakened tsconfig.json:5 strictNullChecks"
        ]
    );
}

#[test]
fn module_resolution_is_the_kinds() {
    let resolved = |module: &str, resolution: &str| {
        format!(
            "{{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"compilerOptions\": {{\n    \"module\": \"{module}\",\n    \"moduleResolution\": \"{resolution}\"\n  }}\n}}\n"
        )
    };
    assert_eq!(
        member("frontend", "apps/web", &[("tsconfig.json", EXTENDS_BASE)]),
        ["resolution tsconfig.json moduleResolution"]
    );
    let node = resolved("NodeNext", "NodeNext");
    assert_eq!(
        member("frontend", "apps/web", &[("tsconfig.json", &node)]),
        [
            "resolution tsconfig.json:4 module",
            "resolution tsconfig.json:5 moduleResolution"
        ]
    );
    let bundler = resolved("ESNext", "bundler");
    assert_eq!(
        member("frontend", "apps/web", &[("tsconfig.json", &bundler)]),
        Vec::<String>::new()
    );
    assert_eq!(
        member("service", "apps/s", &[("tsconfig.json", &bundler)]),
        [
            "resolution tsconfig.json:4 module",
            "resolution tsconfig.json:5 moduleResolution"
        ]
    );
    assert_eq!(
        member("service", "apps/s", &[("tsconfig.json", EXTENDS_BASE)]),
        Vec::<String>::new()
    );
    let inherits = "{ \"extends\": \"./tsconfig.base.json\" }";
    let owner = |own: &str| {
        member(
            "frontend",
            "",
            &[("tsconfig.base.json", BASE), ("tsconfig.json", own)],
        )
    };
    assert_eq!(
        owner(inherits),
        ["resolution tsconfig.json moduleResolution"]
    );
    let explicit =
        resolved("ESNext", "Bundler").replace("../../tsconfig.base.json", "./tsconfig.base.json");
    assert_eq!(owner(&explicit), Vec::<String>::new());
}

#[test]
fn the_base_path_is_an_option() {
    let files = [
        ("config/tsconfig.base.json", BASE),
        (
            "tsconfig.json",
            "{\n  \"extends\": \"./config/tsconfig.base.json\"\n}\n",
        ),
    ];
    let moved = Fixture::new("service", &files)
        .plugin_options(r#"{"kind": "service", "base": "config/tsconfig.base.json"}"#)
        .lines(RULE);
    assert_eq!(moved, Vec::<String>::new());
    assert_eq!(
        Fixture::new("service", &files).lines(RULE),
        [
            "copy config/tsconfig.base.json tsconfig.base.json",
            "base config/tsconfig.base.json tsconfig.base.json",
            "base tsconfig.json:2 tsconfig.json"
        ]
    );
}

#[test]
fn a_project_in_a_separate_install_reaches_that_install_s_base() {
    let separate = r#"{"kind": "service", "separate": [{"path": "spikes/load", "job": "a load test with its own install"}]}"#;
    let run = |extends: &str| {
        let own = format!("{{ \"extends\": \"{extends}\" }}");
        Fixture::new(
            "service",
            &[
                ("tsconfig.base.json", BASE),
                ("tsconfig.json", own.as_str()),
            ],
        )
        .plugin_options(separate)
        .project("spikes/load")
        .lines(RULE)
    };
    assert_eq!(run("./tsconfig.base.json"), Vec::<String>::new());
    assert_eq!(
        run("../../tsconfig.base.json"),
        ["base tsconfig.json:1 tsconfig.json"]
    );
}

#[test]
fn a_project_without_typescript_has_nothing_to_check() {
    assert_eq!(
        member("frontend", "apps/web", &[("package.json", "{}")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_root_project_leaves_its_members_configurations_to_them() {
    let web = "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"compilerOptions\": { \"moduleResolution\": \"Bundler\" }\n}\n";
    let files = [
        ("tsconfig.base.json", BASE),
        ("tsconfig.json", "{ \"extends\": \"./tsconfig.base.json\" }"),
        ("apps/web/package.json", "{}"),
        ("apps/web/tsconfig.json", web),
    ];
    assert_eq!(member("monorepo-root", "", &files), Vec::<String>::new());
}

#[test]
fn extends_resolves_as_typescript_resolves_it() {
    let files = [
        ("tsconfig.json", "{ \"extends\": \"./configs\" }"),
        (
            "configs/tsconfig.json",
            "{ \"extends\": \"../../../tsconfig.base\" }",
        ),
        ("tsconfig.build.json", "{ \"extends\": \"./tsconfig\" }"),
    ];
    assert_eq!(member("service", "apps/s", &files), Vec::<String>::new());
    let listed = [(
        "tsconfig.json",
        "{ \"extends\": [\"@tsconfig/strictest\", \"../../tsconfig.base.json\"] }",
    )];
    assert_eq!(member("service", "apps/s", &listed), Vec::<String>::new());
    let missing = [("tsconfig.json", "{ \"extends\": \"./configs\" }")];
    assert_eq!(
        member("service", "apps/s", &missing),
        ["base tsconfig.json:1 tsconfig.json"]
    );
}

#[test]
fn a_solution_configuration_is_checked_through_its_references() {
    let solution = "{\n  \"files\": [],\n  \"references\": [{ \"path\": \"./tsconfig.app.json\" }, { \"path\": \"tsconfig.node.json\" }]\n}\n";
    let bundler = "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"include\": [\"src\"],\n  \"compilerOptions\": { \"module\": \"ESNext\", \"moduleResolution\": \"Bundler\" }\n}\n";
    let files = [
        ("tsconfig.json", solution),
        ("tsconfig.app.json", bundler),
        ("tsconfig.node.json", bundler),
    ];
    assert_eq!(member("frontend", "apps/web", &files), Vec::<String>::new());
    let inherits = [
        ("tsconfig.json", solution),
        ("tsconfig.app.json", bundler),
        ("tsconfig.node.json", EXTENDS_BASE),
    ];
    assert_eq!(
        member("frontend", "apps/web", &inherits),
        ["resolution tsconfig.node.json moduleResolution"]
    );
    let compiles = solution.replace("\"files\": []", "\"files\": [\"main.ts\"]");
    assert_eq!(
        member("service", "apps/s", &[("tsconfig.json", &compiles)]),
        ["base tsconfig.json tsconfig.json"]
    );
}

#[test]
fn a_chain_that_leaves_the_project_says_so_and_names_the_base() {
    let files = [(
        "tsconfig.json",
        "{ \"extends\": \"../../packages/config/tsconfig.json\" }",
    )];
    let fixture = Fixture::new("service", &files).project("apps/s");
    let findings = fixture.check(RULE, "{}").expect("the rule runs");
    assert_eq!(lines(&findings), ["base tsconfig.json:1 tsconfig.json"]);
    assert!(
        findings[0]
            .message
            .contains("extends through packages/config/tsconfig.json, outside the project"),
        "{}",
        findings[0].message
    );
    assert_eq!(
        findings[0].help.as_deref(),
        Some("extend the base directly: \"extends\": \"../../tsconfig.base.json\"")
    );
}

#[test]
fn a_base_inside_a_member_package_is_left_to_tsconfig_base() {
    let weak_base = BASE.replace("\"strict\": true", "\"strict\": false");
    let files = [
        (
            "tsconfig.json",
            "{ \"extends\": \"./packages/tsconfig/tsconfig.base.json\" }",
        ),
        ("packages/tsconfig/package.json", "{}"),
        ("packages/tsconfig/tsconfig.base.json", weak_base.as_str()),
    ];
    let options = r#"{"kind": "monorepo-root", "base": "packages/tsconfig/tsconfig.base.json"}"#;
    let root = Fixture::new("monorepo-root", &files).plugin_options(options);
    assert_eq!(root.lines(RULE), Vec::<String>::new());
    let package = Fixture::new(
        "library",
        &[
            ("tsconfig.base.json", weak_base.as_str()),
            ("package.json", "{}"),
        ],
    )
    .plugin_options(&options.replace("monorepo-root", "library"))
    .project("packages/tsconfig");
    assert_eq!(package.lines(RULE), Vec::<String>::new());
    assert_eq!(
        root.lines("tsconfig-base"),
        ["flags packages/tsconfig/tsconfig.base.json:3 strict"]
    );
}

#[test]
fn files_a_reads_grant_adds_from_outside_the_project_are_not_its_own() {
    let weak =
        "{\n  \"compilerOptions\": { \"strict\": false, \"moduleResolution\": \"Bundler\" }\n}\n";
    let files = [
        ("tsconfig.json", EXTENDS_BASE),
        ("../../docs/shared/tsconfig.json", weak),
    ];
    assert_eq!(member("service", "apps/s", &files), Vec::<String>::new());
}

#[test]
fn a_chain_passes_through_a_json_file_of_any_name() {
    let files = [
        ("tsconfig.json", "{ \"extends\": \"./config/base.json\" }"),
        (
            "config/base.json",
            "{ \"extends\": \"../../../tsconfig.base.json\" }",
        ),
    ];
    assert_eq!(member("service", "apps/s", &files), Vec::<String>::new());
    let broken = [
        ("tsconfig.json", "{ \"extends\": \"./config/base.json\" }"),
        ("config/base.json", "{ \"compilerOptions\": {} }"),
    ];
    assert_eq!(
        member("service", "apps/s", &broken),
        ["base tsconfig.json:1 tsconfig.json"]
    );
}

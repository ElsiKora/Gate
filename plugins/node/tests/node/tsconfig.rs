use crate::fixture::Fixture;

const RULE: &str = "tsconfig";

/// The standard base: every standard flag on (lines 3 to 12) and the common resolution (13, 14).
const BASE: &str = r#"{
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
fn the_project_that_owns_the_base_checks_its_flags() {
    let root = [
        ("tsconfig.base.json", BASE),
        ("tsconfig.json", "{ \"extends\": \"./tsconfig.base.json\" }"),
    ];
    assert_eq!(member("monorepo-root", "", &root), Vec::<String>::new());
    let lacking = BASE.replace("    \"verbatimModuleSyntax\": true,\n", "");
    let files = [("tsconfig.base.json", lacking.as_str())];
    assert_eq!(
        member("monorepo-root", "", &files),
        ["flags tsconfig.base.json verbatimModuleSyntax"]
    );
    let bundled = BASE.replace("\"module\": \"NodeNext\"", "\"module\": \"ESNext\"");
    let files = [("tsconfig.base.json", bundled.as_str())];
    assert_eq!(
        member("monorepo-root", "", &files),
        ["flags tsconfig.base.json:13 module"]
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
    let fixture = Fixture::new("service", &files);
    let moved = fixture
        .check(RULE, r#"{"base": "config/tsconfig.base.json"}"#)
        .expect("runs");
    assert_eq!(crate::fixture::lines(&moved), Vec::<String>::new());
    assert_eq!(
        fixture.lines(RULE),
        [
            "copy config/tsconfig.base.json tsconfig.base.json",
            "base config/tsconfig.base.json tsconfig.base.json",
            "base tsconfig.json:2 tsconfig.json",
            "flags tsconfig.base.json tsconfig.base.json"
        ]
    );
}

#[test]
fn a_project_without_typescript_has_nothing_to_check() {
    assert_eq!(
        member("frontend", "apps/web", &[("package.json", "{}")]),
        Vec::<String>::new()
    );
}

use crate::fixture::Fixture;

const RULE: &str = "linter";

const ROOT: &str = "import { createConfig } from \"@elsikora/eslint-config\";\n\nexport default await createConfig({ withTypescript: true });\n";
const MEMBER: &str = "import root from \"../../eslint.config.mjs\";\n\nexport default root;\n";
const MANIFEST: &str = r#"{"devDependencies": {"eslint": "10.15.0"}}"#;

fn run(root: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![
        ("package.json", MANIFEST),
        ("eslint.config.mjs", root),
        ("apps/a/eslint.config.mjs", MEMBER),
    ];
    files.extend_from_slice(extra);
    Fixture::new("monorepo-root", &files).lines(RULE)
}

#[test]
fn a_root_on_the_shared_configuration_imported_by_its_members_passes() {
    assert_eq!(run(ROOT, &[]), Vec::<String>::new());
}

#[test]
fn the_root_extends_the_shared_configuration_and_no_other_base() {
    let own = "import js from \"@eslint/js\";\n\nexport default [js.configs.recommended];\n";
    assert_eq!(
        run(own, &[]),
        ["shared eslint.config.mjs @elsikora/eslint-config"]
    );
    let commented =
        format!("// import {{ createConfig }} from \"@elsikora/eslint-config\";\n{own}");
    assert_eq!(
        run(&commented, &[]),
        ["shared eslint.config.mjs @elsikora/eslint-config"]
    );
    let quoted = format!("const note = \"import from '@elsikora/eslint-config'\";\n{own}");
    assert_eq!(
        run(&quoted, &[]),
        ["shared eslint.config.mjs @elsikora/eslint-config"]
    );
    let two = ROOT.replace("\n\n", "\nimport other from \"eslint-config-other\";\n");
    assert_eq!(
        run(&two, &[]),
        ["shared eslint.config.mjs:2 eslint-config-other"]
    );
}

#[test]
fn the_shared_package_is_an_option() {
    let scoped = "import { base } from \"@scope/lint/flat\";\n\nexport default base;\n";
    let files = [("package.json", MANIFEST), ("eslint.config.mjs", scoped)];
    let fixture = Fixture::new("service", &files);
    let lines = |options: &str| crate::fixture::lines(&fixture.check(RULE, options).expect("runs"));
    assert_eq!(lines(r#"{"shared": "@scope/lint"}"#), Vec::<String>::new());
    assert_eq!(
        lines("{}"),
        ["shared eslint.config.mjs @elsikora/eslint-config"]
    );
    let mixed = Fixture::new(
        "service",
        &[("package.json", MANIFEST), ("eslint.config.mjs", ROOT)],
    );
    assert_eq!(
        crate::fixture::lines(
            &mixed
                .check(RULE, r#"{"shared": "@scope/lint"}"#)
                .expect("runs")
        ),
        [
            "shared eslint.config.mjs @scope/lint",
            "shared eslint.config.mjs:1 @elsikora/eslint-config"
        ]
    );
}

#[test]
fn legacy_configurations_other_names_and_detached_members_are_reported() {
    let extra = [
        ("apps/b/.eslintrc.json", "{}"),
        (
            "apps/c/eslint.config.mjs",
            "import { createConfig } from \"@elsikora/eslint-config\";\nexport default await createConfig({});\n",
        ),
        ("apps/d/eslint.config.js", MEMBER),
        (
            "apps/e/package.json",
            "{\n  \"name\": \"e\",\n  \"eslintConfig\": {}\n}\n",
        ),
    ];
    assert_eq!(
        run(ROOT, &extra),
        [
            "legacy apps/e/package.json:3 eslintConfig",
            "legacy apps/b/.eslintrc.json .eslintrc.json",
            "detached apps/c/eslint.config.mjs eslint.config.mjs",
            "root apps/d/eslint.config.js eslint.config.js"
        ]
    );
}

#[test]
fn a_repository_that_uses_eslint_has_the_root_configuration() {
    assert_eq!(
        Fixture::new("service", &[("package.json", MANIFEST)]).lines(RULE),
        ["root eslint.config.mjs eslint.config.mjs"]
    );
    assert_eq!(
        Fixture::new("service", &[("package.json", "{}")]).lines(RULE),
        Vec::<String>::new()
    );
}

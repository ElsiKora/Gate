use crate::fixture::{Fixture, message};

const RULE: &str = "scripts";

/// Every standard script, one project script and an npm lifecycle script, on lines 3 to 11.
const BASE: [(&str, &str); 9] = [
    ("build", "tsc -p tsconfig.build.json"),
    ("lint", "eslint . --max-warnings 0"),
    ("lint:fix", "eslint . --fix"),
    ("typecheck", "tsc --noEmit"),
    ("test", "vitest run"),
    ("format", "prettier --check ."),
    ("format:fix", "prettier --write ."),
    ("start:api", "node dist/main.js"),
    ("prepublishOnly", "pnpm run build"),
];

fn manifest(scripts: &[(&str, &str)]) -> String {
    let lines: Vec<String> = scripts
        .iter()
        .map(|(name, command)| format!("    \"{name}\": \"{command}\""))
        .collect();
    format!("{{\n  \"scripts\": {{\n{}\n  }}\n}}\n", lines.join(",\n"))
}

fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut scripts = BASE.to_vec();
    scripts.extend_from_slice(extra);
    scripts
}

/// The findings for a project of `kind` with `scripts`; `root` adds the lockfile of an install
/// root, `typescript` a `tsconfig.json`.
fn run(kind: &str, scripts: &[(&str, &str)], root: bool, typescript: bool) -> Vec<String> {
    let manifest = manifest(scripts);
    let mut files = vec![("package.json", manifest.as_str())];
    if root {
        files.push(("pnpm-lock.yaml", ""));
    }
    if typescript {
        files.push(("tsconfig.json", "{}"));
    }
    Fixture::new(kind, &files).lines(RULE)
}

#[test]
fn the_standard_scripts_pass() {
    assert_eq!(run("service", &BASE, true, true), Vec::<String>::new());
    assert_eq!(
        run("frontend", &with(&[("dev", "vite")]), true, true),
        Vec::<String>::new()
    );
}

#[test]
fn a_synonym_of_a_standard_name_is_reported_with_that_name() {
    let scripts = with(&[
        ("lint:types", "tsc --noEmit"),
        ("format:check", "prettier --check ."),
        ("prettier:fix", "prettier --write ."),
        ("lint:ts", "eslint ."),
    ]);
    assert_eq!(
        run("service", &scripts, true, true),
        [
            "synonym package.json:12 lint:types",
            "synonym package.json:13 format:check",
            "synonym package.json:14 prettier:fix",
            "synonym package.json:15 lint:ts"
        ]
    );
    let manifest = manifest(&with(&[("lint:types", "tsc --noEmit")]));
    let fixture = Fixture::new(
        "service",
        &[("package.json", &manifest), ("pnpm-lock.yaml", "")],
    );
    assert_eq!(
        message(&fixture, RULE),
        "script lint:types does the job of the standard script typecheck"
    );
}

#[test]
fn a_bare_name_that_writes_is_reported() {
    let scripts: Vec<(&str, &str)> = BASE
        .iter()
        .map(|(name, command)| {
            if *name == "lint" {
                (*name, "eslint --fix .")
            } else {
                (*name, *command)
            }
        })
        .collect();
    assert_eq!(
        run("service", &scripts, true, true),
        ["writes package.json:4 lint"]
    );
}

#[test]
fn a_workspace_member_formats_nothing_itself() {
    assert_eq!(
        run("library", &BASE, false, true),
        [
            "misplaced package.json:8 format",
            "misplaced package.json:9 format:fix"
        ]
    );
    let without: Vec<(&str, &str)> = BASE
        .iter()
        .copied()
        .filter(|(name, _)| !name.starts_with("format"))
        .collect();
    assert_eq!(run("library", &without, false, true), Vec::<String>::new());
}

#[test]
fn the_kind_and_the_project_decide_what_is_required() {
    assert_eq!(
        run("frontend", &BASE, true, true),
        ["missing package.json:2 dev"]
    );
    let untyped: Vec<(&str, &str)> = BASE
        .iter()
        .copied()
        .filter(|(name, _)| *name != "typecheck")
        .collect();
    assert_eq!(run("service", &untyped, true, false), Vec::<String>::new());
    assert_eq!(
        run("service", &untyped, true, true),
        ["missing package.json:2 typecheck"]
    );
    let empty = Fixture::new(
        "infrastructure",
        &[("package.json", "{}"), ("pnpm-lock.yaml", "")],
    )
    .lines(RULE);
    assert_eq!(
        empty,
        [
            "missing package.json build",
            "missing package.json lint",
            "missing package.json lint:fix",
            "missing package.json test",
            "missing package.json format",
            "missing package.json format:fix"
        ]
    );
}

#[test]
fn a_name_is_kebab_case_segments() {
    assert_eq!(
        run(
            "service",
            &with(&[("startApi", "node dist/main.js")]),
            true,
            true
        ),
        ["name package.json:12 startApi"]
    );
}

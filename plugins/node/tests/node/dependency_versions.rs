use crate::fixture::Fixture;

const RULE: &str = "dependency-versions";

#[test]
fn without_a_workspace_every_installed_version_is_exact_and_peers_may_range() {
    let manifest = r#"{
  "dependencies": {
    "exact": "1.2.3",
    "caret": "^1.2.3",
    "tilde": "~1.2.3",
    "any": "*",
    "tag": "latest"
  },
  "peerDependencies": {
    "react": "^18"
  }
}
"#;
    assert_eq!(
        Fixture::new("library", &[("package.json", manifest)]).lines(RULE),
        [
            "range package.json:4 caret",
            "range package.json:5 tilde",
            "range package.json:6 any",
            "range package.json:7 tag"
        ]
    );
}

#[test]
fn without_a_catalog_one_dependency_at_two_versions_differs_once() {
    let files = [
        (
            "package.json",
            r#"{"devDependencies": {"left-pad": "1.0.0"}}"#,
        ),
        (
            "tools/x/package.json",
            r#"{"devDependencies": {"left-pad": "1.1.0"}}"#,
        ),
        (
            "tools/y/package.json",
            r#"{"dependencies": {"left-pad": "1.0.0"}}"#,
        ),
    ];
    assert_eq!(
        Fixture::new("service", &files).lines(RULE),
        ["differs tools/x/package.json:1 left-pad"]
    );
}

#[test]
fn in_a_workspace_externals_come_from_the_catalog_and_members_through_workspace_star() {
    let member = r#"{
  "name": "a",
  "dependencies": {
    "b": "workspace:*",
    "left-pad": "catalog:",
    "pinned": "1.2.3",
    "c": "workspace:^"
  }
}
"#;
    let files = [
        (
            "pnpm-workspace.yaml",
            "packages:\n  - \"apps/*\"\ncatalog:\n  left-pad: 1.1.0\n  loose: ^2.0.0\n",
        ),
        (
            "package.json",
            r#"{"name": "r", "devDependencies": {"left-pad": "catalog:"}}"#,
        ),
        ("apps/a/package.json", member),
        (
            "apps/b/package.json",
            r#"{"name": "b", "dependencies": {"a": "1.0.0"}}"#,
        ),
    ];
    assert_eq!(
        Fixture::new("monorepo-root", &files).lines(RULE),
        [
            "range pnpm-workspace.yaml:5 loose",
            "catalog apps/a/package.json:6 pinned",
            "internal apps/a/package.json:7 c",
            "internal apps/b/package.json:1 a"
        ]
    );
}

#[test]
fn a_catalog_reference_needs_a_catalog() {
    let files = [
        ("pnpm-workspace.yaml", "packages:\n  - \"apps/*\"\n"),
        (
            "apps/a/package.json",
            r#"{"dependencies": {"x": "catalog:"}}"#,
        ),
    ];
    assert_eq!(
        Fixture::new("monorepo-root", &files).lines(RULE),
        ["catalog apps/a/package.json:1 x"]
    );
}

#[test]
fn a_catalog_reference_names_an_entry_of_that_catalog() {
    let member = r#"{
  "name": "a",
  "dependencies": {
    "left-pad": "catalog:default",
    "react": "catalog:react18",
    "react-dom": "catalog:react18",
    "is-odd": "catalog:"
  }
}
"#;
    let files = [
        (
            "pnpm-workspace.yaml",
            "packages:\n  - \"apps/*\"\ncatalog:\n  left-pad: 1.1.0\ncatalogs:\n  react18:\n    react: 18.3.1\n",
        ),
        ("apps/a/package.json", member),
    ];
    let fixture = Fixture::new("monorepo-root", &files);
    assert_eq!(
        fixture.lines(RULE),
        [
            "catalog apps/a/package.json:6 react-dom",
            "catalog apps/a/package.json:7 is-odd"
        ]
    );
}

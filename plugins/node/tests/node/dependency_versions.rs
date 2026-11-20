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

#[test]
fn a_workspace_file_without_packages_is_no_workspace() {
    let settings = "onlyBuiltDependencies:\n  - esbuild\n";
    let exact = r#"{"name": "app", "dependencies": {"zod": "4.1.12"}}"#;
    assert_eq!(
        Fixture::new(
            "service",
            &[("pnpm-workspace.yaml", settings), ("package.json", exact)]
        )
        .lines(RULE),
        Vec::<String>::new()
    );
    let ranged = r#"{"name": "app", "dependencies": {"zod": "^4.1.12"}}"#;
    assert_eq!(
        Fixture::new(
            "service",
            &[("pnpm-workspace.yaml", settings), ("package.json", ranged)]
        )
        .lines(RULE),
        ["range package.json:1 zod"]
    );
}

/// In a workspace the root's own dependencies come from the catalog too: one version per
/// dependency across the install, the root's tools included.
#[test]
fn in_a_workspace_the_root_takes_its_dependencies_from_the_catalog_too() {
    let files = [
        (
            "pnpm-workspace.yaml",
            "packages:\n  - \"apps/*\"\ncatalog:\n  eslint: 10.15.0\n",
        ),
        (
            "package.json",
            "{\n  \"name\": \"r\",\n  \"devDependencies\": {\n    \"eslint\": \"10.15.0\"\n  }\n}\n",
        ),
    ];
    assert_eq!(
        Fixture::new("monorepo-root", &files).lines(RULE),
        ["catalog package.json:4 eslint"]
    );
}

#[test]
fn aliases_local_packages_and_pinned_git_urls_are_read_by_their_kind() {
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let manifest = format!(
        r#"{{
  "dependencies": {{
    "alias": "npm:left-pad@1.3.0",
    "scoped-alias": "npm:@scope/pad@1.3.0",
    "loose-alias": "npm:@scope/pad@^1",
    "local": "file:../x",
    "linked": "link:../y",
    "own": "workspace:*",
    "pinned-git": "github:owner/repo#{commit}",
    "pinned-url": "git+https://example.test/r.git#commit={commit}",
    "branch": "git+https://example.test/r.git#main",
    "short": "owner/repo",
    "caret": "^1.2.3",
    "catalogued": "catalog:"
  }}
}}
"#
    );
    let findings = Fixture::new("library", &[("package.json", &manifest)])
        .check(RULE, "{}")
        .expect("the rule runs");
    let found: Vec<(String, String)> = findings
        .iter()
        .map(|finding| {
            (
                finding.key.clone().unwrap_or_default(),
                finding.help.clone().unwrap_or_default(),
            )
        })
        .collect();
    let expected = [
        (
            "loose-alias",
            "write npm:@scope/pad@x.y.z with the exact version that is installed",
        ),
        (
            "branch",
            "pin it to a full 40-character commit: git+https://example.test/r.git#<commit>",
        ),
        (
            "short",
            "pin it to a full 40-character commit: owner/repo#<commit>",
        ),
        (
            "caret",
            "write the exact version x.y.z that is installed; save-exact=true keeps it so",
        ),
        (
            "catalogued",
            "catalog: needs a pnpm workspace that declares the catalog; write the exact version x.y.z here",
        ),
    ];
    assert_eq!(
        found,
        expected.map(|(key, help)| (key.to_owned(), help.to_owned()))
    );
}

#[test]
fn local_packages_need_no_catalog_entry_in_a_workspace() {
    let files = [
        (
            "pnpm-workspace.yaml",
            "packages:\n  - apps/*\ncatalog:\n  left-pad: 1.3.0\n",
        ),
        ("package.json", r#"{"name": "root"}"#),
        (
            "apps/a/package.json",
            r#"{"name": "a", "dependencies": {"left-pad": "catalog:", "x": "file:../../vendor/x", "y": "link:../../vendor/y"}}"#,
        ),
    ];
    assert_eq!(
        Fixture::new("monorepo-root", &files).lines(RULE),
        Vec::<String>::new()
    );
}

#[test]
fn each_install_root_keeps_its_own_versions() {
    let files = [
        (
            "package.json",
            r#"{"devDependencies": {"left-pad": "1.0.0"}}"#,
        ),
        (
            "spikes/load/package.json",
            r#"{"devDependencies": {"left-pad": "1.1.0"}}"#,
        ),
    ];
    let separate = r#"{"kind": "service", "separate": [{"path": "spikes/load", "job": "a load test with its own install"}]}"#;
    assert_eq!(
        Fixture::new("service", &files)
            .plugin_options(separate)
            .lines(RULE),
        Vec::<String>::new()
    );
    assert_eq!(
        Fixture::new("service", &files).lines(RULE),
        ["differs spikes/load/package.json:1 left-pad"]
    );
}

#[test]
fn a_manifest_with_a_byte_order_mark_is_read() {
    let manifest = "\u{feff}{\n  \"dependencies\": {\n    \"caret\": \"^1.2.3\"\n  }\n}\n";
    assert_eq!(
        Fixture::new("library", &[("package.json", manifest)]).lines(RULE),
        ["range package.json:3 caret"]
    );
}

#[test]
fn a_bare_local_path_is_read_as_a_path_not_a_git_url() {
    let manifest = r#"{"dependencies": {"up": "../x", "here": "./vendor/y"}}"#;
    let findings = Fixture::new("library", &[("package.json", manifest)])
        .check(RULE, "{}")
        .expect("the rule runs");
    let helps: Vec<&str> = findings
        .iter()
        .map(|finding| finding.help.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(
        helps,
        [
            "write file:../x; a local package is named with the file: protocol",
            "write file:./vendor/y; a local package is named with the file: protocol"
        ]
    );
    let files = [
        (
            "pnpm-workspace.yaml",
            "packages:\n  - apps/*\ncatalog:\n  left-pad: 1.3.0\n",
        ),
        ("package.json", r#"{"name": "root"}"#),
        (
            "apps/a/package.json",
            r#"{"name": "a", "dependencies": {"x": "../../vendor/x"}}"#,
        ),
    ];
    assert_eq!(
        Fixture::new("monorepo-root", &files).lines(RULE),
        ["range apps/a/package.json:1 x"]
    );
}

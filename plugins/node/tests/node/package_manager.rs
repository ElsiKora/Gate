use crate::fixture::Fixture;

const RULE: &str = "package-manager";

/// A pnpm workspace with one member, pinned and exact.
fn workspace(
    manifest: &str,
    extra: &[(&'static str, &'static str)],
) -> Vec<(&'static str, String)> {
    let mut files = vec![
        ("package.json", manifest.to_owned()),
        ("pnpm-lock.yaml", String::new()),
        (
            ".npmrc",
            "registry=https://registry.example.test/\nsave-exact = true\n".to_owned(),
        ),
        (
            "pnpm-workspace.yaml",
            "packages:\n  - \"apps/*\"\n  - '!apps/legacy'\n".to_owned(),
        ),
        ("apps/a/package.json", "{\"name\": \"a\"}".to_owned()),
    ];
    files.extend(extra.iter().map(|(path, text)| (*path, (*text).to_owned())));
    files
}

fn run(files: &[(&str, String)]) -> Vec<String> {
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    Fixture::new("monorepo-root", &files).lines(RULE)
}

const PINNED: &str = "{\n  \"name\": \"r\",\n  \"packageManager\": \"pnpm@9.1.0\"\n}\n";

#[test]
fn a_pinned_pnpm_workspace_passes() {
    assert_eq!(run(&workspace(PINNED, &[])), Vec::<String>::new());
    let hashed = "{\"packageManager\": \"pnpm@9.1.0+sha512.0123abcd\"}";
    assert_eq!(run(&workspace(hashed, &[])), Vec::<String>::new());
}

#[test]
fn package_manager_is_pnpm_at_one_exact_version() {
    for manifest in [
        "{\n  \"name\": \"r\",\n  \"packageManager\": \"pnpm@^9\"\n}\n",
        "{\n  \"name\": \"r\",\n  \"packageManager\": \"npm@10.0.0\"\n}\n",
        "{\n  \"name\": \"r\",\n  \"packageManager\": \"pnpm@9.1\"\n}\n",
    ] {
        assert_eq!(
            run(&workspace(manifest, &[])),
            ["field package.json:3 packageManager"],
            "{manifest}"
        );
    }
    assert_eq!(
        run(&workspace("{\"name\": \"r\"}", &[])),
        ["field package.json packageManager"]
    );
}

#[test]
fn another_managers_lockfile_anywhere_and_a_missing_pnpm_lockfile_are_reported() {
    let files = workspace(
        PINNED,
        &[("apps/a/package-lock.json", "{}"), ("yarn.lock", "")],
    );
    assert_eq!(
        run(&files),
        [
            "lockfile apps/a/package-lock.json package-lock.json",
            "lockfile yarn.lock yarn.lock"
        ]
    );
    let without: Vec<(&str, String)> = workspace(PINNED, &[])
        .into_iter()
        .filter(|(path, _)| *path != "pnpm-lock.yaml")
        .collect();
    assert_eq!(run(&without), ["lockfile pnpm-lock.yaml pnpm-lock.yaml"]);
}

#[test]
fn a_nested_lockfile_is_reported_unless_its_directory_is_declared_separate() {
    let spike = [
        (
            "spikes/x/package.json",
            "{\"packageManager\": \"pnpm@9.1.0\"}",
        ),
        ("spikes/x/pnpm-lock.yaml", ""),
        ("spikes/x/.npmrc", "save-exact=true\n"),
    ];
    let files = workspace(PINNED, &spike);
    assert_eq!(
        run(&files),
        [
            "nested spikes/x/pnpm-lock.yaml pnpm-lock.yaml",
            "members spikes/x/package.json package.json"
        ]
    );
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    let separate = Fixture::new("monorepo-root", &files)
        .plugin_options(r#"{"kind": "monorepo-root", "separate": [{"path": "spikes/x", "job": "a measurement spike with its own install"}]}"#)
        .lines(RULE);
    assert_eq!(separate, Vec::<String>::new());
}

#[test]
fn a_manifest_outside_every_package_glob_is_reported() {
    let files = workspace(
        PINNED,
        &[
            ("tools/x/package.json", "{}"),
            ("apps/legacy/package.json", "{}"),
        ],
    );
    assert_eq!(
        run(&files),
        [
            "members apps/legacy/package.json package.json",
            "members tools/x/package.json package.json"
        ]
    );
}

#[test]
fn installs_keep_exact_versions_through_save_exact() {
    let files: Vec<(&str, String)> = workspace(PINNED, &[])
        .into_iter()
        .map(|(path, text)| {
            if path == ".npmrc" {
                (path, "save-exact=false\n".to_owned())
            } else {
                (path, text)
            }
        })
        .collect();
    assert_eq!(run(&files), ["save-exact .npmrc save-exact"]);
    let with = |npmrc: &str, settings: &str| {
        let files: Vec<(&str, String)> = workspace(PINNED, &[])
            .into_iter()
            .map(|(path, text)| match path {
                ".npmrc" => (path, npmrc.to_owned()),
                "pnpm-workspace.yaml" => (path, format!("{text}{settings}")),
                _ => (path, text),
            })
            .collect();
        run(&files)
    };
    let none = Vec::<String>::new();
    let finding = ["save-exact .npmrc save-exact"];
    assert_eq!(with("save-exact=true\nsave-exact=false\n", ""), finding);
    assert_eq!(with("save-exact=false\nsave-exact=true\n", ""), none);
    assert_eq!(with("# save-exact=true\n", ""), finding);
    assert_eq!(with("save-prefix=\n", ""), none);
    assert_eq!(with("save-prefix=\"\"\n", ""), none);
    assert_eq!(with("save-prefix=^\n", ""), finding);
    assert_eq!(with("", "saveExact: true\n"), none);
    assert_eq!(with("", "savePrefix: ''\n"), none);
    assert_eq!(with("save-exact=true\n", "saveExact: false\n"), finding);
    let settings_only = [
        ("package.json", PINNED.to_owned()),
        ("pnpm-lock.yaml", String::new()),
        (
            "pnpm-workspace.yaml",
            "onlyBuiltDependencies:\n  - esbuild\nsaveExact: true\n".to_owned(),
        ),
    ];
    assert_eq!(run(&settings_only), none);
}

#[test]
fn a_repository_without_a_manifest_is_not_a_node_repository() {
    assert_eq!(
        Fixture::new("service", &[("README.md", "# x\n")]).lines(RULE),
        Vec::<String>::new()
    );
}

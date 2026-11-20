//! The `[plugins.nestjs] options` layout block: defaults, configuration errors, alias resolution
//! and the schema the configuration is checked against.

use gate_sdk::serde_json::{self, Value};
use nestjs::layout::{Layout, PluginOptions};

fn options(json: &str) -> Result<PluginOptions, serde_json::Error> {
    serde_json::from_str(json)
}

#[test]
fn an_empty_table_is_the_conventional_layout() {
    let layout = options("{}").expect("parses").layout;

    assert_eq!(layout, Layout::default());
    assert_eq!(layout.source_root, "src");
    assert_eq!(layout.modules_root, "src/modules");
    assert_eq!(
        layout.path_aliases.get("@modules").map(String::as_str),
        Some("src/modules")
    );
    assert_eq!(layout.path_aliases.len(), layout.source_owners.len());
    assert_eq!(
        layout.planes_without_domain().collect::<Vec<_>>(),
        ["integration", "internal", "public"]
    );
}

#[test]
fn a_key_that_is_set_replaces_only_that_default() {
    let layout = options(r#"{ "layout": { "test-root": "spec", "kind-folders": ["dto"] } }"#)
        .expect("parses")
        .layout;

    assert_eq!(layout.test_root, "spec");
    assert_eq!(layout.kind_folders, ["dto"]);
    assert_eq!(layout.source_root, "src");
    assert_eq!(layout.modules_root, "src/modules");
}

#[test]
fn the_folders_below_the_source_root_derive_from_it() {
    let layout = options(r#"{ "layout": { "source-root": "app" } }"#)
        .expect("parses")
        .layout;

    assert_eq!(layout.modules_root, "app/modules");
    assert_eq!(layout.infrastructure_folder, "app/infrastructure");
    assert_eq!(
        layout.path_aliases.get("@modules").map(String::as_str),
        Some("app/modules")
    );
    assert_eq!(layout.test_root, "test");

    let root = options(r#"{ "layout": { "source-root": "", "source-owners": ["shared"] } }"#)
        .expect("parses")
        .layout;
    assert_eq!(root.modules_root, "modules");
    assert_eq!(root.infrastructure_folder, "infrastructure");
    assert_eq!(
        root.resolve_alias("@shared/a/b").as_deref(),
        Some("shared/a/b")
    );
}

#[test]
fn a_derived_folder_that_is_set_keeps_its_value() {
    let layout = options(
        r#"{ "layout": { "source-root": "app", "modules-root": "features", "path-aliases": { "~": "app" } } }"#,
    )
    .expect("parses")
    .layout;

    assert_eq!(layout.modules_root, "features");
    assert_eq!(layout.infrastructure_folder, "app/infrastructure");
    assert_eq!(layout.path_aliases.len(), 1);
    assert_eq!(layout.resolve_alias("~/x").as_deref(), Some("app/x"));
}

#[test]
fn a_folder_name_with_a_slash_is_an_error() {
    let layout = options(r#"{ "layout": { "kind-folders": ["dto/"] } }"#)
        .expect("parses")
        .layout;
    let error = layout
        .check_paths()
        .expect_err("a name has no slash")
        .to_string();

    assert!(
        error.contains("`layout.kind-folders[0]`") && error.contains(r#"write "dto""#),
        "{error}"
    );
}

#[test]
fn an_unknown_key_is_an_error_that_names_it() {
    let error = options(r#"{ "layout": { "source_root": "app" } }"#)
        .expect_err("an unknown key fails")
        .to_string();
    assert!(error.contains("source_root"), "{error}");

    let error = options(r#"{ "layout": { "skip-folders": ["dist"] } }"#)
        .expect_err("the skip list is a language setting, not a layout key")
        .to_string();
    assert!(error.contains("skip-folders"), "{error}");

    let error = options(r#"{ "layout": { "cdk-root": "infrastructure/cdk" } }"#)
        .expect_err("a CDK app is the cdk plugin's project, not a layout key")
        .to_string();
    assert!(error.contains("cdk-root"), "{error}");

    let error = options(r#"{ "lay-out": {} }"#)
        .expect_err("an unknown key fails")
        .to_string();
    assert!(error.contains("lay-out"), "{error}");
}

#[test]
fn a_value_of_the_wrong_type_is_an_error() {
    let error = options(r#"{ "layout": { "module-planes": "domain" } }"#)
        .expect_err("a string is not a list")
        .to_string();
    assert!(error.contains("sequence"), "{error}");
}

#[test]
fn an_alias_matches_itself_or_a_path_below_it() {
    let layout = Layout::default();

    assert_eq!(
        layout.resolve_alias("@modules").as_deref(),
        Some("src/modules")
    );
    assert_eq!(
        layout.resolve_alias("@modules/user/user.module").as_deref(),
        Some("src/modules/user/user.module")
    );
    assert_eq!(layout.resolve_alias("@modulesx/user"), None);
    assert_eq!(layout.resolve_alias("./modules"), None);
}

#[test]
fn the_longer_of_two_matching_aliases_wins() {
    let layout = options(
        r#"{ "layout": { "path-aliases": { "@app": "src", "@app/core": "src/core-lib" } } }"#,
    )
    .expect("parses")
    .layout;

    assert_eq!(
        layout.resolve_alias("@app/core/a").as_deref(),
        Some("src/core-lib/a")
    );
    assert_eq!(
        layout.resolve_alias("@app/other").as_deref(),
        Some("src/other")
    );
}

#[test]
fn the_schema_closes_the_layout_table_over_its_keys() {
    let schema = serde_json::to_value(gate_sdk::schemars::schema_for!(PluginOptions))
        .expect("the schema serializes");
    let layout = &schema["$defs"]["Layout"];

    assert_eq!(layout["additionalProperties"], Value::Bool(false));
    let mut keys: Vec<&str> = layout["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "artifact-folders",
            "automator-package",
            "domain-folder",
            "exclusive-kind-folders",
            "generated-routes",
            "infrastructure-folder",
            "kind-folders",
            "module-planes",
            "modules-root",
            "nest-common-package",
            "nest-core-package",
            "path-aliases",
            "role-folders",
            "scripts-root",
            "source-owners",
            "source-root",
            "swagger-package",
            "test-root",
            "test-scenario-suffixes",
            "test-support-folders",
            "test-support-kinds",
            "typeorm-column-decorators",
            "typeorm-entity-decorators",
            "typeorm-other-persisted-decorators",
            "typeorm-package",
            "typeorm-relation-decorators",
            "validator-package",
        ]
    );
}

#[test]
fn the_source_prefix_is_empty_for_the_project_root() {
    let prefix = |json: &str| options(json).expect("parses").layout.source_prefix();

    assert_eq!(prefix("{}"), "src/");
    assert_eq!(
        prefix(r#"{ "layout": { "source-root": "app/server" } }"#),
        "app/server/"
    );
    assert_eq!(prefix(r#"{ "layout": { "source-root": "" } }"#), "");
}

use crate::fixture::{Fixture, message};

const RULE: &str = "package";

const SERVICE: &str = r#"{
  "name": "svc",
  "version": "1.0.0",
  "private": true,
  "license": "MIT",
  "type": "module",
  "engines": { "node": ">=24.11.0 <25" }
}
"#;

const LIBRARY: &str = r#"{
  "name": "@scope/lib",
  "version": "1.0.0",
  "license": "MIT",
  "type": "module",
  "engines": { "node": ">=24.11.0 <25" },
  "publishConfig": { "access": "public" },
  "exports": { ".": "./dist/index.js" },
  "files": ["dist"],
  "sideEffects": false,
  "repository": "github:example/lib"
}
"#;

fn run(kind: &str, manifest: &str) -> Vec<String> {
    Fixture::new(kind, &[("package.json", manifest)]).lines(RULE)
}

#[test]
fn each_kind_passes_with_its_fields() {
    for kind in ["service", "frontend", "infrastructure", "monorepo-root"] {
        assert_eq!(run(kind, SERVICE), Vec::<String>::new(), "{kind}");
    }
    assert_eq!(run("library", LIBRARY), Vec::<String>::new());
}

#[test]
fn a_library_is_published_and_nothing_else_is() {
    let public = SERVICE.replace("  \"private\": true,\n", "");
    assert_eq!(
        run("library", &public),
        [
            "missing package.json publishConfig",
            "missing package.json exports",
            "missing package.json files",
            "missing package.json sideEffects",
            "missing package.json repository"
        ]
    );
    assert_eq!(
        run("service", LIBRARY),
        [
            "missing package.json private",
            "forbidden package.json:8 exports",
            "forbidden package.json:9 files",
            "forbidden package.json:10 sideEffects"
        ]
    );
}

#[test]
fn a_private_library_is_a_workspace_package_that_still_has_exports() {
    let internal = LIBRARY.replace("\"version\"", "\"private\": true,\n  \"version\"");
    assert_eq!(run("library", &internal), Vec::<String>::new());
    assert_eq!(run("library", SERVICE), ["missing package.json exports"]);
}

#[test]
fn a_service_may_say_where_its_source_lives() {
    let with_repository = SERVICE.replace(
        "\"type\": \"module\",",
        "\"type\": \"module\",\n  \"repository\": \"github:example/svc\",",
    );
    assert_eq!(run("service", &with_repository), Vec::<String>::new());
}

#[test]
fn every_kind_is_an_es_module_that_declares_its_node() {
    let untyped = SERVICE.replace("  \"type\": \"module\",\n", "");
    assert_eq!(run("service", &untyped), ["missing package.json type"]);
    let commonjs = SERVICE.replace("\"module\"", "\"commonjs\"");
    assert_eq!(
        run("infrastructure", &commonjs),
        ["value package.json:6 type"]
    );
    let fixture = Fixture::new("infrastructure", &[("package.json", &commonjs)]);
    assert_eq!(
        message(&fixture, RULE),
        "package.json sets type to \"commonjs\"; every kind is \"module\""
    );
    let engineless = SERVICE.replace(",\n  \"engines\": { \"node\": \">=24.11.0 <25\" }", "");
    assert_eq!(
        run("frontend", &engineless),
        ["missing package.json engines.node"]
    );
}

#[test]
fn a_public_project_and_a_workspaces_key_are_reported() {
    let public = SERVICE
        .replace("\"private\": true", "\"private\": false")
        .replace(
            "\"license\"",
            "\"workspaces\": [\"apps/*\"],\n  \"license\"",
        );
    assert_eq!(
        run("service", &public),
        [
            "value package.json:4 private",
            "forbidden package.json:5 workspaces"
        ]
    );
}

#[test]
fn a_project_without_a_manifest_is_reported() {
    assert_eq!(
        Fixture::new("service", &[("src/main.ts", "")]).lines(RULE),
        ["missing package.json package.json"]
    );
}

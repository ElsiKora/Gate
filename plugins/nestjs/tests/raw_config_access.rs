//! `nestjs/raw-config-access` against synthetic modules that reach a configuration package.

mod support;

use support::{Row, check, error, row};

const RULE: &str = "raw-config-access";
const RAW: &str = r#"{
    "raw-imports": [{ "module": "@scope/config", "names": ["ConfigService", "SecretService"] }],
    "approved-files": ["src/app.config.ts"],
    "approved-prefixes": ["src/config/"],
    "approved-segments": ["/runtime-config/"]
}"#;
const SERVICE: &str = "src/modules/user/user.service.ts";

fn raw(path: &str) -> Row {
    row(
        "",
        path,
        None,
        &format!("{path} imports a raw config service outside an approved config boundary"),
    )
}

#[test]
fn a_named_import_outside_the_boundary_is_reported() {
    let text = "import { ConfigService } from \"@scope/config\";\n";
    let files = [
        (SERVICE, text),
        ("src/app.config.ts", text),
        ("src/config/loader.ts", text),
        ("src/modules/user/runtime-config/user.ts", text),
        ("src/node_modules/pkg/index.ts", text),
        ("test/user.test.ts", text),
    ];

    assert_eq!(check(RULE, RAW, &files), [raw(SERVICE)]);
}

#[test]
fn other_names_and_subpaths_of_the_module_are_not_raw() {
    let text = "import { ConfigModule } from \"@scope/config\";\n\
                import { ConfigService } from \"@scope/config/testing\";\n\
                export const MODULES = [ConfigModule, ConfigService];\n";

    assert_eq!(check(RULE, RAW, &[(SERVICE, text)]), []);
}

#[test]
fn names_are_followed_through_source_re_exports() {
    let barrel = "export { ConfigService as Settings } from \"@scope/config\";\n\
                  export const OTHER = 1;\n";
    let star = "export * from \"../config\";\n";
    let files = [
        ("src/config/index.ts", barrel),
        ("src/shared/all.ts", star),
        (
            "src/modules/user/user.service.ts",
            "import { Settings } from \"../../config\";\nexport class UserService { constructor(readonly settings: Settings) {} }\n",
        ),
        (
            "src/modules/user/user.helper.ts",
            "import { OTHER } from \"../../shared/all\";\nexport const HELPER = OTHER;\n",
        ),
        (
            "src/modules/user/user.reader.ts",
            "import * as all from \"../../shared/all\";\nexport type Reader = all.Settings;\n",
        ),
    ];

    assert_eq!(
        check(RULE, RAW, &files),
        [
            raw("src/modules/user/user.reader.ts"),
            raw("src/modules/user/user.service.ts"),
            raw("src/shared/all.ts")
        ]
    );
}

#[test]
fn require_dynamic_import_and_import_types_reach_the_module() {
    let required = "const config = require(\"@scope/config\");\n\
                    export const make = () => new config.SecretService();\n";
    let awaited = "export async function load() {\n\
                   \x20 const { ConfigService } = await import(\"@scope/config\");\n\
                   \x20 return ConfigService;\n\
                   }\n";
    let typed = "export type Service = import(\"@scope/config\").ConfigService;\n";
    let unrelated = "export type Module = import(\"@scope/config\").ConfigModule;\n";

    assert_eq!(
        check(
            RULE,
            RAW,
            &[
                ("src/a.ts", required),
                ("src/b.ts", awaited),
                ("src/c.ts", typed),
                ("src/d.ts", unrelated),
            ]
        ),
        [raw("src/a.ts"), raw("src/b.ts"), raw("src/c.ts")]
    );
}

#[test]
fn malformed_options_are_rejected() {
    let message = error(
        RULE,
        r#"{ "raw-imports": [{ "module": "@scope/config" }] }"#,
        &[],
    );
    assert!(message.contains("names"), "{message}");

    let message = error(RULE, r#"{ "approvedFiles": [] }"#, &[]);
    assert!(message.contains("approvedFiles"), "{message}");
}

#[test]
fn a_missing_or_empty_raw_imports_option_is_an_error() {
    let missing = error(RULE, "{}", &[]);
    assert!(missing.contains("raw-imports"), "{missing}");

    let empty = error(RULE, r#"{ "raw-imports": [] }"#, &[]);
    assert!(
        empty.contains("option `raw-imports` lists nothing"),
        "{empty}"
    );
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "import { ConfigService } from \"@scope/config\";\n";
    let generated = "src/modules/generated/user/user.service.ts";
    let vendored = "src/node_modules/pkg/index.ts";
    let files = [(generated, text), (vendored, text)];
    let paths = |languages: &str| support::paths_with(RULE, RAW, "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

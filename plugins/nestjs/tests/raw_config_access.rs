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
        "boundary",
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

/// The options of the coverage tests: the boundary of `RAW`, and `NODE_OPTIONS` read by Node.js.
const COVERAGE: &str = r#"{
    "raw-imports": [{ "module": "@scope/config", "names": ["ConfigService"] }],
    "approved-prefixes": ["src/config/"],
    "external-keys": [{ "key": "NODE_OPTIONS", "reader": "Node.js" }]
}"#;
const CONFIG: &str = "src/config/app.config.ts";
/// A value of the example file, which no finding may hold.
const VALUE: &str = "s3cr3t-placeholder";

/// The rows of `aspect`, as path and line, with every row checked for `VALUE`.
fn of(rows: &[Row], aspect: &str) -> Vec<(String, Option<u32>)> {
    for (_, _, _, message) in rows {
        assert!(!message.contains(VALUE), "a value in {message:?}");
    }
    rows.iter()
        .filter(|(name, ..)| name == aspect)
        .map(|(_, path, line, _)| (path.clone(), *line))
        .collect()
}

fn at(path: &str, line: u32) -> (String, Option<u32>) {
    (path.to_owned(), Some(line))
}

/// A configuration class in the boundary whose getters read `API_URL` and `API_PORT`, a loop over
/// `LOOPED_A` and `LOOPED_B`, and a factory parameter typed as the service that reads `API_MODE`.
const READER: &str = "import { ConfigService } from \"@scope/config\";\n\
    export class AppConfig {\n\
      constructor(private readonly config: ConfigService) {}\n\
      get url(): string { return this.config.get(\"API_URL\"); }\n\
      get port(): string { return this.config.getOrThrow<string>(\"API_PORT\"); }\n\
      check(): void { for (const key of [\"LOOPED_A\", \"LOOPED_B\"]) { this.config.getOrThrow(key); } }\n\
    }\n\
    export const factory = (config: ConfigService) => config.get(\"API_MODE\");\n";

fn example(keys: &[&str]) -> String {
    keys.iter()
        .map(|key| format!("{key}={VALUE}\n"))
        .collect::<Vec<_>>()
        .concat()
}

const ALL: [&str; 6] = [
    "API_URL",
    "API_PORT",
    "LOOPED_A",
    "LOOPED_B",
    "API_MODE",
    "NODE_OPTIONS",
];

#[test]
fn process_env_outside_the_boundary_is_reported_at_each_read_and_inside_it_passes() {
    let text = "export const a = process.env.API_URL;\n\
                export const b = process.env[\"API_PORT\"];\n\
                const { API_HOST } = process.env;\n\
                export const c = \"API_MODE\" in process.env;\n\
                export function local(process: { env: Record<string, string> }) { return process.env.OTHER; }\n\
                export const d = API_HOST;\n";
    let env = example(&["API_URL", "API_PORT", "API_HOST", "API_MODE"]);
    let files = [
        (SERVICE, text),
        ("src/config/env.ts", text),
        (".env.example", env.as_str()),
    ];
    let rows = check(RULE, COVERAGE, &files);

    assert_eq!(
        of(&rows, "process-env"),
        [
            at(SERVICE, 1),
            at(SERVICE, 2),
            at(SERVICE, 3),
            at(SERVICE, 4)
        ]
    );
    assert_eq!(rows.len(), 4, "{rows:?}");
}

#[test]
fn a_key_a_getter_reads_without_a_line_is_undeclared_and_with_one_passes() {
    let missing = example(&[
        "API_URL",
        "LOOPED_A",
        "LOOPED_B",
        "API_MODE",
        "NODE_OPTIONS",
    ]);
    let rows = check(
        RULE,
        COVERAGE,
        &[(CONFIG, READER), (".env.example", &missing)],
    );
    assert_eq!(of(&rows, "undeclared"), [at(CONFIG, 5)]);
    assert_eq!(rows.len(), 1, "{rows:?}");

    let all = example(&ALL);
    assert_eq!(
        check(RULE, COVERAGE, &[(CONFIG, READER), (".env.example", &all)]),
        []
    );
}

#[test]
fn a_line_nothing_reads_is_unread_unless_a_loop_reads_it_or_external_keys_list_it() {
    let mut keys = ALL.to_vec();
    keys.insert(1, "UNUSED");
    let env = example(&keys);
    let rows = check(RULE, COVERAGE, &[(CONFIG, READER), (".env.example", &env)]);
    assert_eq!(of(&rows, "unread"), [at(".env.example", 2)]);
    assert_eq!(rows.len(), 1, "{rows:?}");
}

#[test]
fn a_key_named_by_a_parameter_is_dynamic_and_one_named_by_a_const_passes() {
    let text = "import { ConfigService } from \"@scope/config\";\n\
                const NAME = \"API_URL\";\n\
                export class Reader {\n\
                  constructor(private readonly config: ConfigService) {}\n\
                  read(name: string): string { return this.config.get(name); }\n\
                  url(): string { return this.config.get(NAME); }\n\
                }\n";
    let env = example(&["API_URL"]);
    let rows = check(RULE, COVERAGE, &[(CONFIG, text), (".env.example", &env)]);
    assert_eq!(of(&rows, "dynamic"), [at(CONFIG, 5)]);
    assert_eq!(rows.len(), 1, "{rows:?}");
}

#[test]
fn reads_without_an_example_file_give_one_finding() {
    let rows = check(RULE, COVERAGE, &[(CONFIG, READER)]);
    assert_eq!(
        rows,
        [row(
            "example-missing",
            ".env.example",
            None,
            ".env.example is not tracked, and the code reads 5 environment variable(s)"
        )]
    );
    assert_eq!(
        check(RULE, COVERAGE, &[(SERVICE, "export const a = 1;\n")]),
        []
    );
}

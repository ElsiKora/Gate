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

/// The line of `text` that holds `marker`, counted from one.
fn line(text: &str, marker: &str) -> u32 {
    let index = text
        .lines()
        .position(|line| line.contains(marker))
        .unwrap_or_else(|| panic!("no line holds {marker:?}"));
    u32::try_from(index + 1).expect("a short fixture")
}

/// The rows of `aspect` in `rows`, sorted.
fn sorted(rows: &[Row], aspect: &str) -> Vec<(String, Option<u32>)> {
    let mut found = of(rows, aspect);
    found.sort();
    found
}

/// Element callbacks over literal arrays of strings, one form per line, each reading the keys its
/// array holds.
const CALLBACKS: &str = "import { ConfigService } from \"@scope/config\";\n\
    const EACH = [\"K_EACH\"] as const;\n\
    const SOME = [\"K_SOME\"];\n\
    export function read(config: ConfigService): unknown[] {\n\
      return [\n\
        Object.fromEntries([\"K_MAP_A\", \"K_MAP_B\"].map((key) => [key, config.getOrThrow(key)])),\n\
        [\"K_FLAT\"].flatMap(function (key) { return [config.get(key)]; }),\n\
        EACH.forEach((key) => config.get(key)),\n\
        [\"K_FILTER\"].filter((key: string) => config.get(key)),\n\
        [\"K_FIND\"].find((key) => config.get(key)),\n\
        [\"K_FIND_INDEX\"].findIndex((key) => config.get(key)),\n\
        [\"K_FIND_LAST\"].findLast((key) => config.get(key)),\n\
        [\"K_FIND_LAST_INDEX\"].findLastIndex((key, index) => config.get(key) && index),\n\
        SOME?.some((key) => config.get(key)),\n\
        [\"K_EVERY\"].every?.(((key) => config.get(key))),\n\
      ];\n\
    }\n";

const CALLBACK_KEYS: [(&str, &str); 11] = [
    ("K_MAP_A", "K_MAP_A"),
    ("K_MAP_B", "K_MAP_A"),
    ("K_FLAT", "K_FLAT"),
    ("K_EACH", "EACH.forEach"),
    ("K_FILTER", "K_FILTER"),
    ("K_FIND", "\"K_FIND\""),
    ("K_FIND_INDEX", "K_FIND_INDEX"),
    ("K_FIND_LAST", "\"K_FIND_LAST\""),
    ("K_FIND_LAST_INDEX", "K_FIND_LAST_INDEX"),
    ("K_SOME", "SOME?.some"),
    ("K_EVERY", "K_EVERY"),
];

#[test]
fn the_element_parameter_of_a_callback_over_a_literal_array_names_its_keys() {
    let keys: Vec<&str> = CALLBACK_KEYS.iter().map(|(key, _)| *key).collect();
    let all = example(&keys);
    assert_eq!(
        check(
            RULE,
            COVERAGE,
            &[(CONFIG, CALLBACKS), (".env.example", &all)]
        ),
        []
    );

    let rows = check(
        RULE,
        COVERAGE,
        &[(CONFIG, CALLBACKS), (".env.example", "NODE_OPTIONS=1\n")],
    );
    let mut expected: Vec<_> = CALLBACK_KEYS
        .iter()
        .map(|(_, marker)| at(CONFIG, line(CALLBACKS, marker)))
        .collect();
    expected.sort();
    assert_eq!(sorted(&rows, "undeclared"), expected);
    assert_eq!(rows.len(), 11, "{rows:?}");
}

/// Callback parameters that are not the element of a literal array of strings, one per line.
const NOT_CALLBACKS: &str = "import { ConfigService } from \"@scope/config\";\n\
    let MUTABLE = [\"K_LET\"];\n\
    export function read(config: ConfigService, names: string[]): unknown[] {\n\
      function one(key: string): unknown { return config.get(key); } // named-declaration\n\
      const byName = (key: string) => config.get(key); // named-arrow\n\
      return [\n\
        [\"K_BY_NAME\"].forEach(one),\n\
        [\"K_BY_NAME\"].forEach(byName),\n\
        [\"K_REDUCE\"].reduce((key) => config.get(key), \"\"), // accumulator\n\
        [\"K_REDUCE_RIGHT\"].reduceRight((key) => config.get(key), \"\"), // right-accumulator\n\
        [\"K_SECOND\"].map((item, key) => config.get(key)), // index\n\
        names.map((key) => config.get(key)), // names\n\
        MUTABLE.map((key) => config.get(key)), // mutable\n\
        [\"K_DESTRUCTURED\"].map(([key]) => config.get(key)), // destructured\n\
        [\"K_DEFAULT\"].map((key = \"K_DEFAULT\") => config.get(key)), // default\n\
        [\"K_REST\"].map((...key) => config.get(key)), // rest\n\
        [\"K_WRAPPED\"].map(wrap((key: string) => config.get(key))), // wrapped\n\
        [\"K_SECOND_ARGUMENT\"].map(String, (key: string) => config.get(key)), // later-argument\n\
      ];\n\
    }\n\
    declare function wrap<T>(value: T): T;\n";

#[test]
fn a_parameter_that_is_no_element_of_a_literal_array_stays_dynamic() {
    let keys = [
        "K_LET",
        "K_BY_NAME",
        "K_REDUCE",
        "K_REDUCE_RIGHT",
        "K_SECOND",
        "K_DESTRUCTURED",
        "K_DEFAULT",
        "K_REST",
        "K_WRAPPED",
        "K_SECOND_ARGUMENT",
    ];
    let env = example(&keys);
    let rows = check(
        RULE,
        COVERAGE,
        &[(CONFIG, NOT_CALLBACKS), (".env.example", &env)],
    );

    let mut dynamic: Vec<_> = [
        "// named-declaration",
        "// named-arrow",
        "// accumulator",
        "// right-accumulator",
        "// index",
        "// names",
        "// mutable",
        "// destructured",
        "// default",
        "// rest",
        "// wrapped",
        "// later-argument",
    ]
    .iter()
    .map(|marker| at(CONFIG, line(NOT_CALLBACKS, marker)))
    .collect();
    dynamic.sort();
    assert_eq!(sorted(&rows, "dynamic"), dynamic);

    let unread: Vec<_> = (1..=10).map(|index| at(".env.example", index)).collect();
    assert_eq!(sorted(&rows, "unread"), unread);
    assert_eq!(rows.len(), 22, "{rows:?}");
}

/// Getter calls whose first argument can never be a key, and some whose argument is unknown.
const NEVER_KEYS: &str = "import { ConfigService } from \"@scope/config\";\n\
    export function read(store: ConfigService, item: { name: string }, section: string): unknown[] {\n\
      return [\n\
        store.get({ name: item.name, section }),\n\
        store.get([\"API_URL\"]),\n\
        store.get(() => \"API_URL\"),\n\
        store.get(function () { return \"API_URL\"; }),\n\
        store.get(class {}),\n\
        store.getOrThrow(1),\n\
        store.get(1n),\n\
        store.get(true),\n\
        store.get(null),\n\
        store.get(/API_URL/),\n\
        store.get(({ section } as never)!),\n\
        store.get(([] satisfies unknown[]) as never),\n\
        store.get(\"API_URL\"),\n\
        store.get(section), // identifier\n\
        store.get(item.name), // member\n\
        store.get(String(section)), // call\n\
        store.get(section ? \"API_URL\" : \"API_PORT\"), // conditional\n\
        store.get(-1), // unary\n\
        process.env[1], // process-env\n\
      ];\n\
    }\n";

#[test]
fn a_getter_whose_argument_can_never_be_a_key_is_no_read_and_others_stay_dynamic() {
    let env = example(&["API_URL"]);
    let rows = check(
        RULE,
        COVERAGE,
        &[(CONFIG, NEVER_KEYS), (".env.example", &env)],
    );
    let dynamic: Vec<_> = [
        "// identifier",
        "// member",
        "// call",
        "// conditional",
        "// unary",
        "// process-env",
    ]
    .iter()
    .map(|marker| at(CONFIG, line(NEVER_KEYS, marker)))
    .collect();
    assert_eq!(sorted(&rows, "dynamic"), dynamic);
    assert_eq!(rows.len(), 6, "{rows:?}");

    let only = "import { ConfigService } from \"@scope/config\";\n\
                export const read = (store: ConfigService, section: string) => store.get({ section });\n";
    assert_eq!(
        check(RULE, COVERAGE, &[(CONFIG, only), (".env.example", "")]),
        []
    );
    assert_eq!(
        check(RULE, COVERAGE, &[(SERVICE, only), (".env.example", "")]),
        [raw(SERVICE)]
    );
    let unread = check(RULE, COVERAGE, &[(CONFIG, only), (".env.example", &env)]);
    assert_eq!(of(&unread, "unread"), [at(".env.example", 1)]);
    assert_eq!(unread.len(), 1, "{unread:?}");
}

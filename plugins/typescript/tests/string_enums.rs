//! `typescript/string-enums` against synthetic trees: every tracked `.ts` file is read, a file
//! below a test root is a test, and the options change what is reported.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// A fixture tree: tracked paths and their text, no git.
struct Fixture {
    files: BTreeMap<String, String>,
}

impl TreeHost for Fixture {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).map(|text| text.as_bytes().to_vec())
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Fixture {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// `[languages]` with `skip-folders` set to `["generated"]` only: a custom list replaces the
/// default, so `node_modules` is read.
const SKIP_GENERATED: &str = r#"{ "typescript": { "skip-folders": ["generated"] } }"#;

fn run(options: &str, languages: &str, files: &[(&str, &str)]) -> gate_sdk::Result<Vec<Finding>> {
    let fixture = Fixture {
        files: files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect(),
    };
    let request = CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: RULE.to_owned(),
            options: options.to_owned(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: "{}".to_owned(),
        languages: languages.to_owned(),
        norms: Vec::new(),
    };
    gate_sdk::check(&typescript::PLUGIN, &fixture, &request)
}

/// One finding as the tests compare it: aspect, path, line and message.
type Row = (String, String, Option<u32>, String);

fn rows(findings: Vec<Finding>) -> Vec<Row> {
    findings
        .into_iter()
        .map(|finding| {
            let Some(Location::File(file)) = finding.location else {
                panic!("every finding names a file: {:?}", finding.location);
            };
            (
                finding.aspect,
                file.path,
                file.span.map(|span| span.line),
                finding.message,
            )
        })
        .collect()
}

fn check(options: &str, files: &[(&str, &str)]) -> Vec<Row> {
    rows(run(options, "{}", files).expect("the rule runs"))
}

fn error(options: &str) -> String {
    run(options, "{}", &[])
        .expect_err("the rule fails")
        .message()
        .to_owned()
}

/// The paths the findings name, in report order, with consecutive repeats kept once.
fn paths(languages: &str, files: &[(&str, &str)]) -> Vec<String> {
    let mut paths: Vec<String> = rows(run("{}", languages, files).expect("the rule runs"))
        .into_iter()
        .map(|(_, path, _, _)| path)
        .collect();
    paths.dedup();
    paths
}

fn row(aspect: &str, path: &str, line: Option<u32>, message: &str) -> Row {
    (aspect.to_owned(), path.to_owned(), line, message.to_owned())
}

const RULE: &str = "string-enums";
const SOURCE: &str = "src/modules/user/user.type.ts";
const TEST: &str = "test/user/user.case.ts";

fn union(path: &str, line: u32) -> Row {
    let aspect = if path.starts_with("test/") {
        "test-literal-union"
    } else {
        "literal-union"
    };

    row(
        aspect,
        path,
        Some(line),
        "uses a literal union type; use an enum instead",
    )
}

#[test]
fn enums_key_lists_and_single_values_pass() {
    let text = "export enum EState { ACTIVE = \"ACTIVE\", IDLE = \"IDLE\" }\n\
                export interface IUser { state: EState; kind: \"user\"; name?: \"a\" | undefined }\n\
                export type TPicked = Pick<IUser, \"state\" | \"kind\">;\n\
                export type TByKey = Record<\"a\" | \"b\", number>;\n\
                export const STATE = EState.ACTIVE;\n";

    assert_eq!(check("{}", &[(SOURCE, text)]), []);
}

#[test]
fn literal_unions_are_reported_once_per_line_in_every_file() {
    let text = "export type TMode = \"read\" | \"write\";\n\
                export type TPair = { a: \"x\" | \"y\"; b: \"z\" | \"w\" };\n";
    let files = [
        (SOURCE, text),
        (TEST, text),
        ("lib/app.ts", text),
        ("scripts/build.ts", text),
        ("src/modules/user/user.d.ts", text),
        ("src/node_modules/pkg/index.ts", text),
    ];

    assert_eq!(
        check("{}", &files),
        [
            union("lib/app.ts", 1),
            union("lib/app.ts", 2),
            union("scripts/build.ts", 1),
            union("scripts/build.ts", 2),
            union(SOURCE, 1),
            union(SOURCE, 2),
            union(TEST, 1),
            union(TEST, 2),
        ]
    );
}

#[test]
fn identity_keyof_aliases_and_raw_enum_values_are_reported_outside_tests() {
    let text = "export enum EState { ACTIVE = \"ACTIVE\", IDLE = \"IDLE\" }\n\
                export type TState = keyof typeof EState;\n\
                export const CURRENT = \"ACTIVE\";\n";

    assert_eq!(
        check("{}", &[(SOURCE, text), (TEST, text)]),
        [
            row(
                "keyof-typeof-alias",
                SOURCE,
                Some(2),
                "uses a standalone keyof typeof alias; use the enum type directly"
            ),
            row(
                "raw-enum-value",
                SOURCE,
                Some(3),
                "repeats raw string enum value \"ACTIVE\" owned by EState.ACTIVE; use the enum \
                 member instead"
            ),
        ]
    );
}

#[test]
fn a_discriminant_with_several_values_is_reported_at_each_value() {
    let text = "export interface IFirst { kind: \"first\" }\n\
                export interface ISecond { kind: \"second\" }\n";
    let discriminant = |line, value: &str| {
        row(
            "raw-discriminant",
            SOURCE,
            Some(line),
            &format!(
                "uses raw multi-value \"kind\" discriminant \"{value}\"; use an enum member instead"
            ),
        )
    };

    assert_eq!(
        check("{}", &[(SOURCE, text), (TEST, text)]),
        [discriminant(1, "first"), discriminant(2, "second")]
    );
}

#[test]
fn unresolved_namespace_forms_are_reported() {
    let text = "declare module \"m\" {}\n";
    let namespace = |path| {
        row(
            "unsupported-namespace",
            path,
            Some(1),
            "uses the external module declaration \"m\", a namespace form typescript/string-enums \
             does not resolve; remove it",
        )
    };

    assert_eq!(
        check("{}", &[(SOURCE, text), (TEST, text)]),
        [namespace(SOURCE), namespace(TEST)]
    );
}

#[test]
fn test_roots_name_the_test_folders() {
    let text = "export type TMode = \"read\" | \"write\";\n";
    let spec = "apps/api/spec/user.case.ts";
    let message = "uses a literal union type; use an enum instead";

    assert_eq!(
        check(
            r#"{ "test-roots": ["apps/api/spec"] }"#,
            &[(spec, text), (TEST, text)]
        ),
        [
            row("test-literal-union", spec, Some(1), message),
            row("literal-union", TEST, Some(1), message),
        ]
    );

    for root in ["", "/test", "test/", "../test", "a//b"] {
        let message = error(&format!(r#"{{ "test-roots": ["{root}"] }}"#));
        assert!(
            message.starts_with("rule string-enums: test-roots names"),
            "{root}: {message}"
        );
    }
}

#[test]
fn names_and_key_arguments_are_options() {
    let options = r#"{
        "discriminant-names": ["variant"],
        "key-type-arguments": { "Choose": 1 }
    }"#;
    let text = "export interface IA { variant: \"a\"; kind: \"x\" }\n\
                export interface IB { variant: \"b\"; kind: \"y\" }\n\
                export type TKeys = Choose<IA, \"variant\" | \"kind\">;\n\
                export type TPicked = Pick<IA, \"variant\" | \"kind\">;\n";

    assert_eq!(
        check(options, &[(SOURCE, text)]),
        [
            row(
                "raw-discriminant",
                SOURCE,
                Some(1),
                "uses raw multi-value \"variant\" discriminant \"a\"; use an enum member instead"
            ),
            row(
                "raw-discriminant",
                SOURCE,
                Some(2),
                "uses raw multi-value \"variant\" discriminant \"b\"; use an enum member instead"
            ),
            union(SOURCE, 4),
        ]
    );

    let message = error(r#"{ "aliasDepth": 2 }"#);
    assert!(message.contains("aliasDepth"), "{message}");
    let message = error(r#"{ "shim-enum-names": ["ELocal"] }"#);
    assert!(message.contains("shim-enum-names"), "{message}");
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "export type TMode = \"read\" | \"write\";\n";
    let library = "lib/generated/stack.ts";
    let source = "src/modules/generated/user/user.type.ts";
    let scenario = "test/generated/user/user.case.ts";
    let vendored = "lib/node_modules/pkg/stack.ts";
    let files = [
        (library, text),
        (source, text),
        (scenario, text),
        (vendored, text),
    ];

    assert_eq!(paths("{}", &files), [library, source, scenario]);
    assert_eq!(paths(SKIP_GENERATED, &files), [vendored]);
}

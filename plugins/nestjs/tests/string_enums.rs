//! `nestjs/string-enums` against synthetic source, test and infrastructure files.

mod support;

use support::{Row, check, error, row};

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

    assert_eq!(check(RULE, "{}", &[(SOURCE, text)]), []);
}

#[test]
fn literal_unions_are_reported_once_per_line_and_under_their_root() {
    let text = "export type TMode = \"read\" | \"write\";\n\
                export type TPair = { a: \"x\" | \"y\"; b: \"z\" | \"w\" };\n";
    let files = [
        (SOURCE, text),
        (TEST, text),
        ("infrastructure/cdk/lib/stack.ts", text),
        ("infrastructure/cdk/other/stack.ts", text),
        ("src/modules/user/user.d.ts", text),
        ("src/node_modules/pkg/index.ts", text),
    ];

    assert_eq!(
        check(RULE, "{}", &files),
        [
            union("infrastructure/cdk/lib/stack.ts", 1),
            union("infrastructure/cdk/lib/stack.ts", 2),
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
        check(RULE, "{}", &[(SOURCE, text), (TEST, text)]),
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
        check(RULE, "{}", &[(SOURCE, text), (TEST, text)]),
        [discriminant(1, "first"), discriminant(2, "second")]
    );
}

#[test]
fn unresolved_namespace_forms_and_shim_enums_are_reported() {
    let text = "declare module \"m\" {}\n\
                export enum EErrorStringAction { CREATE = \"CREATE\" }\n";
    let namespace = |path| {
        row(
            "unsupported-namespace",
            path,
            Some(1),
            "uses the external module declaration \"m\", a namespace form nestjs/string-enums \
             does not resolve; remove it",
        )
    };

    assert_eq!(
        check(RULE, "{}", &[(SOURCE, text), (TEST, text)]),
        [
            namespace(SOURCE),
            row(
                "error-string-shim",
                SOURCE,
                Some(2),
                &format!(
                    "{SOURCE} declares EErrorStringAction, a local copy of the installed Crud \
                     Automator enum; use its installed members and record a missing action as a \
                     follow-up"
                )
            ),
            namespace(TEST),
        ]
    );
}

#[test]
fn names_and_key_arguments_are_options() {
    let options = r#"{
        "discriminant-names": ["variant"],
        "shim-enum-names": ["ELocal"],
        "key-type-arguments": { "Choose": 1 }
    }"#;
    let text = "export interface IA { variant: \"a\"; kind: \"x\" }\n\
                export interface IB { variant: \"b\"; kind: \"y\" }\n\
                export type TKeys = Choose<IA, \"variant\" | \"kind\">;\n\
                export type TPicked = Pick<IA, \"variant\" | \"kind\">;\n\
                export enum ELocal { A = 1 }\n";

    assert_eq!(
        check(RULE, options, &[(SOURCE, text)]),
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
            row(
                "error-string-shim",
                SOURCE,
                Some(5),
                &format!(
                    "{SOURCE} declares ELocal, a local copy of the installed Crud Automator enum; \
                     use its installed members and record a missing action as a follow-up"
                )
            ),
        ]
    );

    let message = error(RULE, r#"{ "aliasDepth": 2 }"#, &[]);
    assert!(message.contains("aliasDepth"), "{message}");
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "export type TMode = \"read\" | \"write\";\n";
    let cdk = "infrastructure/cdk/lib/generated/stack.ts";
    let source = "src/modules/generated/user/user.type.ts";
    let scenario = "test/generated/user/user.case.ts";
    let vendored = "infrastructure/cdk/lib/node_modules/pkg/stack.ts";
    let files = [
        (cdk, text),
        (source, text),
        (scenario, text),
        (vendored, text),
    ];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [cdk, source, scenario]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

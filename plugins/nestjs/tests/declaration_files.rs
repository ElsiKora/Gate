//! `nestjs/declaration-files` against synthetic source, test and infrastructure files.

mod support;

use support::{Row, check, error, row};

const RULE: &str = "declaration-files";
const SERVICE: &str = "src/modules/user/user.service.ts";

fn owners(path: &str, list: &str) -> String {
    format!(
        "{path} contains multiple code owners: {list}. Named classes, interfaces, types and enums \
         require separate owning files regardless of export visibility; cohesive private functions \
         may remain local."
    )
}

fn misplaced(path: &str, line: u32, name: &str, folder: &str) -> Row {
    row(
        "kind-folder-declaration",
        path,
        Some(line),
        &format!(
            "{path} declares {name}; {folder} contracts belong in the owning {folder}/ folder with \
             a .{folder}.ts filename"
        ),
    )
}

#[test]
fn one_owner_per_file_and_contracts_in_their_kind_folder_pass() {
    let files = [
        (
            SERVICE,
            "function helper(): number { return 1; }\nexport class UserService { run() { return helper(); } }\n",
        ),
        (
            "src/modules/user/type/user.type.ts",
            "export type UserType = { name: string };\n",
        ),
        (
            "src/modules/user/type/x/type/deep.type.ts",
            "export type Deep = string;\n",
        ),
        (
            "src/modules/user/enum/enum.ts",
            "export enum Role { A = \"a\" }\n",
        ),
        (
            "src/modules/user/index.ts",
            "export class A {}\nexport class B {}\n",
        ),
        (
            "test/helper/run.ts",
            "export class A {}\nexport class B {}\n",
        ),
    ];

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn a_second_owner_is_reported_with_every_entity_in_order() {
    let text =
        "export class UserService {}\ninterface Row { id: string }\nexport default class {}\n";

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, text)]),
        [
            row(
                "single-owner",
                SERVICE,
                None,
                &owners(
                    SERVICE,
                    "ClassDeclaration UserService, ClassDeclaration <anonymous>, \
                     InterfaceDeclaration Row"
                )
            ),
            misplaced(SERVICE, 2, "Row", "interface"),
        ]
    );
}

#[test]
fn a_nested_contract_is_reported_at_its_line() {
    let text = "export function make() {\n  class Inner {}\n  return Inner;\n}\n";

    assert_eq!(
        check(RULE, "{}", &[(SERVICE, text)]),
        [
            row(
                "single-owner",
                SERVICE,
                None,
                &owners(SERVICE, "FunctionDeclaration make, ClassDeclaration Inner")
            ),
            row(
                "nested-declaration",
                SERVICE,
                Some(2),
                &format!(
                    "{SERVICE} declares nested Inner; named contracts belong at the top level of \
                     their own file"
                )
            ),
        ]
    );
}

#[test]
fn test_support_files_may_hold_several_factories() {
    let path = "test/fixture/user.fixture.ts";
    let two = "export function a(): number { return 1; }\nexport const b = (): number => 2;\n";
    let three = format!("{two}export class C {{}}\n");

    assert_eq!(check(RULE, "{}", &[(path, two)]), []);
    assert_eq!(
        check(RULE, "{}", &[(path, &three)]),
        [row(
            "support-owner",
            path,
            None,
            &owners(
                path,
                "FunctionDeclaration a, FunctionValuedConst b, ClassDeclaration C"
            )
        )]
    );
}

#[test]
fn a_test_scenario_declares_no_contract_or_bound_class() {
    let path = "test/user.test.ts";
    let text = "interface Row { id: string }\n\
                const Bound = class {};\n\
                it(\"runs\", () => new (class {})());\n\
                export function factory(): number { return 1; }\n";
    let scenario = |line, name: &str| {
        row(
            "test-scenario-declaration",
            path,
            Some(line),
            &format!(
                "{path} declares {name} inside a test scenario; named contracts and independently \
                 bound classes require separate owning files"
            ),
        )
    };

    assert_eq!(
        check(RULE, "{}", &[(path, text)]),
        [scenario(1, "Row"), scenario(2, "<anonymous>")]
    );
}

#[test]
fn a_contract_outside_its_last_kind_folder_or_file_name_is_reported() {
    let below = "src/modules/user/dto/type/user.type.ts";
    let named = "src/modules/user/type/user.ts";
    let text = "export type UserType = string;\n";

    assert_eq!(
        check(RULE, "{}", &[(below, text), (named, text)]),
        [
            misplaced(below, 1, "UserType", "type"),
            misplaced(named, 1, "UserType", "type"),
        ]
    );
}

#[test]
fn the_contract_kinds_are_an_option() {
    let options = r#"{ "contract-kinds": { "EnumDeclaration": "enumeration" } }"#;
    let files = [
        (
            "src/modules/user/enumeration/role.enumeration.ts",
            "export enum Role { A = \"a\" }\n",
        ),
        (
            "src/modules/user/user.type.ts",
            "export type UserType = string;\n",
        ),
        (
            "src/modules/user/enum/role.enum.ts",
            "export enum Kind { A = \"a\" }\n",
        ),
    ];

    assert_eq!(
        check(RULE, options, &files),
        [misplaced(
            "src/modules/user/enum/role.enum.ts",
            1,
            "Kind",
            "enumeration"
        )]
    );

    let message = error(RULE, r#"{ "contractKinds": {} }"#, &[]);
    assert!(message.contains("contractKinds"), "{message}");
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let stack = "export class Stack {}\nexport const make = (): number => 1;\n";
    let source = "src/modules/generated/user/user.service.ts";
    let vendored = "src/node_modules/pkg/user.service.ts";
    let files = [(source, stack), (vendored, stack)];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [source]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

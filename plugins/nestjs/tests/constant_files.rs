//! `nestjs/constant-files` against synthetic constant files.

mod support;

use support::{check, error, row};

const RULE: &str = "constant-files";
const PATH: &str = "src/modules/user/constant/user.constant.ts";

fn one(text: &str) -> Vec<support::Row> {
    check(RULE, "{}", &[(PATH, text)])
}

#[test]
fn a_frozen_upper_case_bag_after_its_backing_constants_passes() {
    let text = "import { Other } from \"./other\";\n\
                const LIMIT = 10;\n\
                export const USER_CONSTANT = Object.freeze({ LIMIT, 'MAX_AGE': 3, NAMES: [\"A\"] });\n";

    assert_eq!(one(text), []);
}

#[test]
fn files_outside_the_constant_suffixes_or_the_source_root_are_not_read() {
    let text = "export function helper(): void {}\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("src/modules/user/user.service.ts", text),
                ("test/user.constant.ts", text),
                ("src/node_modules/pkg/a.constant.ts", text),
            ]
        ),
        []
    );
}

#[test]
fn statements_that_are_not_constants_are_shape_findings_in_order() {
    let text = "\"use strict\";\n\
                export function helper(): void {}\n\
                let COUNTER = 1;\n\
                export const USER_CONSTANT = { A: 1 };\n";

    assert_eq!(
        one(text),
        [
            row(
                "shape",
                PATH,
                Some(1),
                &format!("{PATH} contains a non-constant statement in a .constant.ts file")
            ),
            row(
                "shape",
                PATH,
                Some(2),
                &format!("{PATH} contains a non-constant statement in a .constant.ts file")
            ),
            row(
                "shape",
                PATH,
                Some(3),
                &format!("{PATH} contains a non-const variable statement")
            ),
        ]
    );
}

#[test]
fn the_bag_must_be_one_declared_object_literal_without_spreads() {
    assert_eq!(
        one("export const A_CONSTANT = { A: 1 }, B_CONSTANT = { B: 2 };\n"),
        [
            row(
                "shape",
                PATH,
                Some(1),
                &format!(
                    "{PATH} must export exactly one constant declaration per export statement"
                )
            ),
            row(
                "shape",
                PATH,
                None,
                &format!("{PATH} must export exactly one *_CONSTANT object bag")
            ),
        ]
    );
    assert_eq!(
        one("export { OTHER_CONSTANT } from \"./other\";\n"),
        [row(
            "shape",
            PATH,
            None,
            &format!(
                "{PATH} exports OTHER_CONSTANT; exported constant must be declared in the same file"
            )
        )]
    );
    assert_eq!(
        one("export const USER_CONSTANT = [1, 2];\n"),
        [row(
            "shape",
            PATH,
            Some(1),
            &format!(
                "{PATH} exports USER_CONSTANT; exported constant bag must be an object literal"
            )
        )]
    );
    assert_eq!(
        one("const BASE = { A: 1 };\nexport const USER_CONSTANT = { ...BASE, B: 2 };\n"),
        [row(
            "shape",
            PATH,
            Some(2),
            &format!("{PATH} exports USER_CONSTANT; constant bags must not use spread assignments")
        )]
    );
}

#[test]
fn backing_constants_after_the_bag_are_a_shape_finding() {
    assert_eq!(
        one("export const USER_CONSTANT = { A: 1 };\nconst LATE = 2;\n"),
        [row(
            "shape",
            PATH,
            Some(2),
            &format!("{PATH} declares private backing constants after the exported constant bag")
        )]
    );
}

#[test]
fn names_and_keys_must_be_upper_case_and_the_bag_suffixed() {
    let text = "const limit = 10;\nexport const USER_SETTINGS = { maxAge: 3, OK: 1 };\n";

    assert_eq!(
        one(text),
        [
            row(
                "naming",
                PATH,
                Some(1),
                &format!("{PATH} has private backing constant limit; use UPPER_CASE")
            ),
            row(
                "naming",
                PATH,
                Some(2),
                &format!(
                    "{PATH} exports USER_SETTINGS; exported constant bags must end with _CONSTANT"
                )
            ),
            row(
                "naming",
                PATH,
                Some(2),
                &format!("{PATH} exports USER_SETTINGS; object-bag key maxAge must use UPPER_CASE")
            ),
        ]
    );
}

#[test]
fn callable_code_is_found_through_bindings_literals_and_freeze_calls() {
    let text = "function make(): number { return 1; }\n\
                const MAKER = make;\n\
                const WRAPPED = Object.seal([() => 1]);\n\
                export const USER_CONSTANT = { MAKER, WRAPPED, PLAIN: 1, RUN() { return 2; } };\n";
    let callable = |line, name: &str| {
        row(
            "callable",
            PATH,
            Some(line),
            &format!(
                "{PATH} has constant {name} that retains callable code; move the callable to its owning code file"
            ),
        )
    };

    assert_eq!(
        one(text),
        [
            row(
                "shape",
                PATH,
                Some(1),
                &format!("{PATH} contains a non-constant statement in a .constant.ts file")
            ),
            callable(2, "MAKER"),
            callable(3, "WRAPPED"),
            callable(4, "USER_CONSTANT"),
        ]
    );
}

#[test]
fn the_suffixes_and_freeze_calls_are_options() {
    let options = r#"{ "file-suffixes": [".values.ts"], "bag-suffix": "_VALUES", "freeze-calls": ["freeze"] }"#;
    let path = "src/user.values.ts";
    let text = "const WRAPPED = Object.seal([() => 1]);\nexport const USER_VALUES = { WRAPPED };\n";

    assert_eq!(
        check(RULE, options, &[(path, text), (PATH, "let a = 1;\n")]),
        []
    );

    let message = error(RULE, r#"{ "bag_suffix": "_X" }"#, &[]);
    assert!(message.contains("bag_suffix"), "{message}");
}

#[test]
fn a_source_root_at_the_project_root_reads_every_constant_file() {
    let text = "export const LIMIT = 1;\n";
    let findings = support::run(
        RULE,
        "{}",
        r#"{ "layout": { "source-root": "" } }"#,
        &[("modules/user/constant/user.constant.ts", text)],
    )
    .expect("the rule runs");

    let rows = support::rows(&findings);
    assert!(!rows.is_empty(), "the file is read");
    assert!(
        rows.iter()
            .all(|(_, path, _, _)| path == "modules/user/constant/user.constant.ts"),
        "{rows:?}"
    );
}

#[test]
fn a_layout_folder_that_is_not_normalized_is_an_error_naming_the_rule() {
    for (folder, normal) in [("src/", "src"), ("./src", "src"), ("src//app", "src/app")] {
        let message = support::run(
            RULE,
            "{}",
            &format!(r#"{{ "layout": {{ "source-root": "{folder}" }} }}"#),
            &[(PATH, "")],
        )
        .expect_err("the layout is rejected")
        .message()
        .to_owned();

        assert!(message.contains(RULE), "{message}");
        assert!(message.contains("`layout.source-root`"), "{message}");
        assert!(message.contains(&format!("write {normal:?}")), "{message}");
    }
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "export function helper(): void {}\n";
    let generated = "src/modules/generated/user/constant/user.constant.ts";
    let vendored = "src/node_modules/pkg/a.constant.ts";
    let files = [(generated, text), (vendored, text)];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

//! `nestjs/error-string-shim` against synthetic source and test files.

mod support;

use support::{check, error, row};

const RULE: &str = "error-string-shim";
const SOURCE: &str = "src/modules/user/user.enum.ts";

fn shim(name: &str, line: u32) -> support::Row {
    row(
        "",
        SOURCE,
        Some(line),
        &format!(
            "{SOURCE} declares {name}, a local copy of the installed Crud Automator enum; use its \
             installed members and record a missing action as a follow-up"
        ),
    )
}

#[test]
fn a_source_enum_named_like_an_installed_action_enum_is_reported() {
    let text = "export enum EState { ACTIVE = \"ACTIVE\" }\n\
                export enum EErrorStringAction { CREATE = \"CREATE\" }\n";
    let files = [
        (SOURCE, text),
        ("test/user/user.case.ts", text),
        ("src/modules/user/user.d.ts", text),
    ];

    assert_eq!(check(RULE, "{}", &files), [shim("EErrorStringAction", 2)]);
}

#[test]
fn the_names_are_an_option() {
    let text = "export enum EErrorStringAction { CREATE = \"CREATE\" }\n\
                export enum ELocal { A = 1 }\n";

    assert_eq!(
        check(RULE, r#"{ "enum-names": ["ELocal"] }"#, &[(SOURCE, text)]),
        [shim("ELocal", 2)]
    );
    let message = error(RULE, r#"{ "shim-enum-names": [] }"#, &[]);
    assert!(message.contains("shim-enum-names"), "{message}");
}

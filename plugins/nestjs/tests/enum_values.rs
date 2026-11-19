//! `nestjs/enum-values` against synthetic enums.

mod support;

use support::{Row, check, error, row, rows, run, run_with_languages};

const RULE: &str = "enum-values";
const PATH: &str = "src/modules/user/enum/state.enum.ts";

fn snake(line: u32, target: &str, value: &str) -> Row {
    row(
        "",
        PATH,
        Some(line),
        &format!(
            "{PATH} {target} has the value \"{value}\", which is not UPPER_SNAKE; rename it, or keep \
             a value an external party's specification fixes as a policy exception with its \
             decision"
        ),
    )
}

#[test]
fn upper_snake_string_values_and_other_members_pass() {
    let text = "export enum EState {\n\
                \x20 ACTIVE = \"ACTIVE\",\n\
                \x20 ON_HOLD = `ON_HOLD`,\n\
                \x20 V2_BETA = \"V2_BETA\" as const,\n\
                \x20 COUNT = 3,\n\
                \x20 IMPLICIT,\n\
                \x20 COMPUTED = \"a\".length,\n\
                }\n";

    assert_eq!(check(RULE, "{}", &[(PATH, text)]), []);
}

#[test]
fn every_other_string_value_is_reported_with_its_member_target() {
    let text = "export enum EState {\n\
                \x20 active = \"active\",\n\
                \x20 \"on-hold\" = (\"on-hold\"),\n\
                \x20 DOUBLE = \"A__B\",\n\
                \x20 TRAIL = `TRAIL_`,\n\
                \x20 DIGIT = \"1ST\",\n\
                }\n";
    let outside = [
        ("test/state.enum.ts", text),
        ("src/modules/user/enum/state.d.ts", text),
        ("src/node_modules/pkg/state.ts", text),
    ];
    let mut files = vec![(PATH, text)];
    files.extend(outside);
    let findings = run(RULE, "{}", "{}", &files).expect("the rule runs");
    let targets: Vec<_> = findings
        .iter()
        .map(|finding| finding.target.clone().unwrap_or_default())
        .collect();

    assert_eq!(
        rows(&findings),
        [
            snake(2, "EState.active", "active"),
            snake(3, "EState.on-hold", "on-hold"),
            snake(4, "EState.DOUBLE", "A__B"),
            snake(5, "EState.TRAIL", "TRAIL_"),
            snake(6, "EState.DIGIT", "1ST"),
        ]
    );
    assert_eq!(
        targets,
        [
            "EState.active",
            "EState.on-hold",
            "EState.DOUBLE",
            "EState.TRAIL",
            "EState.DIGIT"
        ]
    );
}

#[test]
fn the_value_pattern_is_an_option() {
    let options = r#"{ "value-pattern": "^[a-z]+(-[a-z]+)*$" }"#;
    let text = "export enum EState { A = \"on-hold\", B = \"ON_HOLD\" }\n";

    assert_eq!(
        check(RULE, options, &[(PATH, text)]),
        [row(
            "",
            PATH,
            Some(1),
            &format!(
                "{PATH} EState.B has the value \"ON_HOLD\", which does not match \
                 ^[a-z]+(-[a-z]+)*$; rename it, or keep a value an external party's specification \
                 fixes as a policy exception with its decision"
            )
        )]
    );

    let message = error(RULE, r#"{ "value-pattern": "([" }"#, &[(PATH, text)]);
    assert!(message.contains("value-pattern"), "{message}");
}

#[test]
fn the_walk_skips_the_folders_of_the_typescript_language_settings() {
    let text = "export enum EState {\n  active = \"active\",\n}\n";
    let files = [
        ("src/generated/state.ts", text),
        ("src/node_modules/pkg/state.ts", text),
    ];
    let paths = |languages: &str| -> Vec<String> {
        rows(&run_with_languages(RULE, languages, &files).expect("the rule runs"))
            .into_iter()
            .map(|(_, path, _, _)| path)
            .collect()
    };

    assert_eq!(paths("{}"), ["src/generated/state.ts"]);
    assert_eq!(
        paths(r#"{ "typescript": { "skip-folders": ["generated"] } }"#),
        ["src/node_modules/pkg/state.ts"]
    );
}

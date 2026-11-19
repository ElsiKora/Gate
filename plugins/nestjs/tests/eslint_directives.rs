//! `nestjs/eslint-directives` against synthetic code files.

mod host;

use host::{SKIP_GENERATED, check, error, paths_with, row};

const RULE: &str = "eslint-directives";
const FILE: &str = "src/modules/book/book.service.ts";

#[test]
fn exact_next_line_directives_pass() {
    let text = "// eslint-disable-next-line no-console, @scope/plugin/rule -- prints the banner\n\
                console.log(1);\n\
                // a plain comment\n";

    assert_eq!(check(RULE, "{}", &[(FILE, text), ("main.mjs", text)]), []);
}

#[test]
fn each_malformed_directive_is_reported_once_in_order() {
    let text = "/* eslint-disable no-console */\n\
                const a = 1; // eslint-disable-line no-console\n\
                // eslint-disable no-console\n\
                // eslint-disable-next-line No-Console -- reason\n\
                // eslint-disable-next-line a, a -- reason\n\
                const b = 2;\n\
                // eslint-disable-next-line no-console -- reason\n\
                \n\
                // eslint-disable-next-line no-console -- reason\n\
                // another comment\n\
                const c = 3;\n";

    assert_eq!(
        check(RULE, "{}", &[(FILE, text)]),
        [
            row(
                "next-line-only",
                FILE,
                Some(1),
                "uses a block ESLint directive; only exact next-line directives are allowed"
            ),
            row(
                "placement",
                FILE,
                Some(2),
                "hides an ESLint directive after code"
            ),
            row(
                "syntax",
                FILE,
                Some(3),
                "must use // eslint-disable-next-line <rule-id>[, <rule-id> ...] -- <nonempty reason>"
            ),
            row(
                "syntax",
                FILE,
                Some(4),
                "must use // eslint-disable-next-line <rule-id>[, <rule-id> ...] -- <nonempty reason>"
            ),
            row("syntax", FILE, Some(5), "lists duplicate ESLint rule IDs"),
            row(
                "placement",
                FILE,
                Some(7),
                "does not target the immediate next code line"
            ),
            row(
                "placement",
                FILE,
                Some(9),
                "does not target the immediate next code line"
            ),
        ]
    );
}

#[test]
fn directives_in_ignored_test_code_are_dead() {
    let text = "// eslint-disable-next-line no-console -- reason\nconsole.log(1);\n";
    let dead = "is a dead ESLint directive in repository-ignored test/type code";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("src/book.spec.ts", text),
                ("src/__tests__/book.ts", text),
                ("src/types/book.d.ts", text),
                ("test/book.e2e.ts", text),
            ]
        ),
        [
            row("dead-test-code", "src/__tests__/book.ts", Some(1), dead),
            row("dead-test-code", "src/book.spec.ts", Some(1), dead),
            row("dead-test-code", "src/types/book.d.ts", Some(1), dead),
            row("dead-test-code", "test/book.e2e.ts", Some(1), dead),
        ]
    );
}

#[test]
fn files_outside_the_universe_are_not_read() {
    let text = "/* eslint-disable */\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("lib/book.ts", text),
                ("src/node_modules/x.js", text),
                ("src/book.md", text),
            ]
        ),
        []
    );
}

#[test]
fn options_change_the_patterns_and_test_roots() {
    let text = "// eslint-disable-next-line No-Console -- reason\nconsole.log(1);\n";
    let options = r#"{ "rule-id-pattern": "^[A-Za-z-]+$", "ignored-test-roots": ["src"] }"#;

    assert_eq!(check(RULE, options, &[("scripts/book.ts", text)]), []);
    assert_eq!(
        check(RULE, options, &[(FILE, text)]),
        [row(
            "dead-test-code",
            FILE,
            Some(1),
            "is a dead ESLint directive in repository-ignored test/type code"
        )]
    );
}

#[test]
fn option_errors_are_reported() {
    assert!(error(RULE, r#"{ "rule-id-pattern": "(" }"#, &[]).contains("rule-id-pattern"));
    assert!(error(RULE, r#"{ "patterns": [] }"#, &[]).contains("patterns"));
}

#[test]
fn roots_are_prefixes_that_keep_a_trailing_slash() {
    let message = error(RULE, r#"{ "roots": ["./src/"] }"#, &[]);

    assert!(message.contains("write \"src/\""), "{message}");
    assert!(error(RULE, r#"{ "roots": ["src//dist/"] }"#, &[]).contains("\"src/dist/\""));
    assert!(error(RULE, r#"{ "skip-prefixes": [] }"#, &[]).contains("skip-prefixes"));
}

#[test]
fn the_language_skip_list_decides_which_folders_are_read() {
    let text = "/* eslint-disable */\n";
    let files = [
        ("src/generated/a.ts", text),
        ("src/modules/coverage/b.ts", text),
        ("src/node_modules/x.ts", text),
    ];

    assert_eq!(
        paths_with(RULE, "{}", "", &files),
        ["src/generated/a.ts", "src/modules/coverage/b.ts"]
    );
    assert_eq!(
        paths_with(RULE, "{}", SKIP_GENERATED, &files),
        ["src/modules/coverage/b.ts", "src/node_modules/x.ts"]
    );
}

#[test]
fn test_roots_and_segments_are_single_names() {
    for option in ["ignored-test-roots", "ignored-test-segments"] {
        for value in ["domain/entity", ""] {
            let message = error(RULE, &format!(r#"{{ "{option}": [{value:?}] }}"#), &[]);

            assert!(message.contains(option), "{message}");
            assert!(message.contains("single name"), "{message}");
        }
    }
}

//! `nestjs/entity-table-name` against synthetic entity files.

mod host;

use host::{SKIP_DIST, check, error, paths_with, row, rows, run};

const RULE: &str = "entity-table-name";
const ENTITY: &str = "src/modules/book/book.entity.ts";

fn message(path: &str, column: u32) -> String {
    format!(
        "{path} uses @Entity() at column {column} without an explicit double-quoted table name; use @Entity(\"table_name\")"
    )
}

#[test]
fn every_entity_without_a_double_quoted_first_argument_is_reported() {
    let text = "@Entity()\n\
                export class BookEntity {}\n\
                @Entity(\"book\")\n\
                @Entity( 'book' )\n\
                @Entity(`book`)\n\
                @Entity({ name: \"book\" })\n\
                @Entity(\"\")\n\
                @Entity\n\
                (\n\
                )\n\
                // @Entity()\n\
                @Entity( \"book\" , { schema: \"library\" })\n\
                @Entity(@Entity(\"shelf\"))\n\
                @EntityRepository(BookEntity)\n\
                /* @Entity(\"book\") */ const s = \"@Entity(1)\";\n\
                @Entity(\"open\"\n";

    assert_eq!(
        check(RULE, "{}", &[(ENTITY, text)]),
        [
            row("", ENTITY, Some(1), &message(ENTITY, 1)),
            row("", ENTITY, Some(4), &message(ENTITY, 1)),
            row("", ENTITY, Some(5), &message(ENTITY, 1)),
            row("", ENTITY, Some(6), &message(ENTITY, 1)),
            row("", ENTITY, Some(7), &message(ENTITY, 1)),
            row("", ENTITY, Some(8), &message(ENTITY, 1)),
            row("", ENTITY, Some(11), &message(ENTITY, 4)),
            row("", ENTITY, Some(13), &message(ENTITY, 1)),
            row("", ENTITY, Some(15), &message(ENTITY, 34)),
        ]
    );
}

#[test]
fn two_entities_on_one_line_are_two_findings_with_character_columns() {
    let text = "export {}; @Entity() class A {} @Entity('b') class B {}\n/* é */ @Entity()\n";

    assert_eq!(
        check(RULE, "{}", &[(ENTITY, text)]),
        [
            row("", ENTITY, Some(1), &message(ENTITY, 12)),
            row("", ENTITY, Some(1), &message(ENTITY, 33)),
            row("", ENTITY, Some(2), &message(ENTITY, 9)),
        ]
    );
}

#[test]
fn javascript_white_space_and_crlf_lines_are_read_as_javascript_reads_them() {
    let white_text = "@Entity(\u{feff}\"book\"\u{feff})\n\
                  @Entity(\u{85}\"book\")\n\
                  @Entity\u{2028}(\u{2028}\"book\")\n\
                  @Entity\u{3000}(\"book\", )\n";
    let crlf_text = "@Entity()\r\nclass A {}\r\n@Entity(\r\n  \"book\"\r\n)\r\n@Entity(\r\n)\r\n";
    let white_path = "src/modules/book/white.ts";
    let crlf_path = "src/modules/book/windows.ts";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[(white_path, white_text), (crlf_path, crlf_text)]
        ),
        [
            row("", white_path, Some(2), &message(white_path, 1)),
            row("", crlf_path, Some(1), &message(crlf_path, 1)),
            row("", crlf_path, Some(6), &message(crlf_path, 1)),
        ]
    );
}

#[test]
fn declaration_files_and_every_folder_are_read_but_only_ts_files_below_the_source_root() {
    let unnamed = "@Entity()\n";
    let declaration = "src/types/book.d.ts";
    let vendored = "src/node_modules/library/index.ts";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (declaration, unnamed),
                (vendored, unnamed),
                ("src/modules/.ts", unnamed),
                ("src/modules/book.mts", unnamed),
                ("src/modules/book.js", unnamed),
                ("src/modules/book.md", unnamed),
                ("test/book.entity.ts", unnamed),
            ]
        ),
        [
            row("", vendored, Some(1), &message(vendored, 1)),
            row("", declaration, Some(1), &message(declaration, 1)),
        ]
    );
}

#[test]
fn the_decorator_option_and_the_source_root_change_what_is_read() {
    let path = "lib/book.ts";
    let text = "@Table()\n@Entity()\n";
    let findings = run(
        RULE,
        r#"{ "decorator": "@Table" }"#,
        r#"{ "layout": { "source-root": "lib" } }"#,
        &[(path, text), (ENTITY, text)],
    )
    .expect("the rule runs");

    assert_eq!(
        rows(&findings),
        [row(
            "",
            path,
            Some(1),
            "lib/book.ts uses @Table() at column 1 without an explicit double-quoted table name; use @Table(\"table_name\")"
        )]
    );
}

#[test]
fn an_unknown_or_empty_option_is_an_error() {
    assert!(error(RULE, r#"{ "decorators": [] }"#, &[]).contains("decorators"));
    assert!(error(RULE, r#"{ "decorator": "" }"#, &[]).contains("decorator"));
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let files = [
        ("src/dist/a.entity.ts", "@Entity()\n"),
        ("src/modules/generated/b.entity.ts", "@Entity()\n"),
    ];

    assert_eq!(
        paths_with(RULE, "{}", SKIP_DIST, &files),
        ["src/dist/a.entity.ts", "src/modules/generated/b.entity.ts"]
    );
}

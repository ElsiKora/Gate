//! `nestjs/prefix-nesting` against synthetic source trees.

mod host;

use host::{SKIP_DIST, check, error, paths_with, row, rows, run};

const RULE: &str = "prefix-nesting";
const EMPTY: &str = "export {};\n";

fn message(directory: &str, count: usize, kind: &str, prefix: &str) -> String {
    format!(
        "{directory} has {count} {kind} sharing \"{prefix}\"; nest them under {directory}/{prefix}"
    )
}

fn tree(paths: &[&'static str]) -> Vec<(&'static str, &'static str)> {
    paths.iter().map(|path| (*path, EMPTY)).collect()
}

/// The findings sorted by path and message, as the host reports them.
fn sorted(mut rows: Vec<host::Row>) -> Vec<host::Row> {
    rows.sort();
    rows
}

#[test]
fn resource_folders_sharing_a_prefix_are_reported_at_their_parent() {
    let files = tree(&[
        "src/library/book-shelf/module.ts",
        "src/library/book-cover/controller.ts",
        "src/catalog/author/module.ts",
        "src/catalog/author-profile/module.ts",
        "src/catalog/author-notes/readme.md",
    ]);

    assert_eq!(
        sorted(check(RULE, "{}", &files)),
        [
            row(
                "",
                "src/catalog",
                None,
                &message("src/catalog", 1, "resource directories", "author")
            ),
            row(
                "",
                "src/library",
                None,
                &message("src/library", 2, "resource directories", "book")
            ),
        ]
    );
}

#[test]
fn same_kind_files_sharing_a_prefix_are_reported_per_kind() {
    let files = tree(&[
        "src/library/enum/book-genre.enum.ts",
        "src/library/enum/book-format-kind.enum.ts",
        "src/library/enum/book-genre.type.ts",
        "src/library/enum/author.type.ts",
        "src/library/dto/loan-request-body.dto.ts",
        "src/library/dto/loan-request-query.dto.ts",
        "src/library/dto/loan-response.dto.ts",
    ]);

    assert_eq!(
        sorted(check(RULE, "{}", &files)),
        [
            row(
                "",
                "src/library/dto",
                None,
                &message("src/library/dto", 2, ".dto.ts files", "loan-request")
            ),
            row(
                "",
                "src/library/enum",
                None,
                &message("src/library/enum", 2, ".enum.ts files", "book")
            ),
        ]
    );
}

#[test]
fn an_owner_folder_of_the_same_kind_takes_a_single_file() {
    let files = tree(&[
        "src/shelf/utility/reader/card.planner.ts",
        "src/shelf/utility/reader-history.planner.ts",
        "src/shelf/utility/loan-open.utility.ts",
        "src/shelf/utility/loan/index.ts",
        "src/order/type-header-value.type.ts",
        "src/order/type-internal/index.ts",
        "src/order/type/deep/value.type.ts",
    ]);

    assert_eq!(
        sorted(check(RULE, "{}", &files)),
        [
            row(
                "",
                "src/order",
                None,
                &message("src/order", 1, ".type.ts files", "type")
            ),
            row(
                "",
                "src/shelf/utility",
                None,
                &message("src/shelf/utility", 1, ".planner.ts files", "reader")
            ),
        ]
    );
}

#[test]
fn nested_unrelated_ignored_declared_and_top_level_files_pass() {
    let files = tree(&[
        "src/book/interface/request/interface.ts",
        "src/book/interface/request/options.interface.ts",
        "src/book/utility/encode.utility.ts",
        "src/book/utility/decode.utility.ts",
        "src/book/utility/index.ts",
        "src/book/utility/index.utility.ts",
        "src/book/book.type.ts",
        "src/book/book-key.type.ts",
        "src/book/types.d.ts",
        "src/book/other.d.ts",
        "src/top-one.type.ts",
        "src/top-two.type.ts",
        "src/book/shelf-one/service.ts",
        "src/book/shelf-two/service.ts",
        "lib/shelf/loan-one.type.ts",
        "lib/shelf/loan-two.type.ts",
    ]);

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn folders_named_node_modules_are_walked() {
    let files = tree(&[
        "src/node_modules/shelf/loan-one.type.ts",
        "src/node_modules/shelf/loan-two.type.ts",
    ]);

    assert_eq!(
        check(RULE, "{}", &files),
        [row(
            "",
            "src/node_modules/shelf",
            None,
            &message("src/node_modules/shelf", 2, ".type.ts files", "loan")
        )]
    );
}

#[test]
fn options_change_the_resource_files_ignored_stems_and_separator() {
    let files = tree(&[
        "src/book/shelf-one/service.ts",
        "src/book/shelf-two/service.ts",
        "src/book/utility/index-one.utility.ts",
        "src/book/utility/index-two.utility.ts",
        "src/book/type/loan_one.type.ts",
        "src/book/type/loan_two.type.ts",
    ]);
    let options = r#"{ "resource-files": ["service.ts"], "ignored-stems": ["index-one"], "word-separator": "_" }"#;

    assert_eq!(
        sorted(check(RULE, options, &files)),
        [row(
            "",
            "src/book/type",
            None,
            &message("src/book/type", 2, ".type.ts files", "loan")
        ),]
    );

    let options = r#"{ "resource-files": ["service.ts"] }"#;

    assert_eq!(
        sorted(check(RULE, options, &files)),
        [
            row(
                "",
                "src/book",
                None,
                &message("src/book", 2, "resource directories", "shelf")
            ),
            row(
                "",
                "src/book/utility",
                None,
                &message("src/book/utility", 2, ".utility.ts files", "index")
            ),
        ]
    );
}

#[test]
fn the_layout_source_root_bounds_the_walk() {
    let files = tree(&[
        "app/shelf/loan-one.type.ts",
        "app/shelf/loan-two.type.ts",
        "src/shelf/loan-one.type.ts",
        "src/shelf/loan-two.type.ts",
    ]);
    let findings = run(
        RULE,
        "{}",
        r#"{ "layout": { "source-root": "app" } }"#,
        &files,
    )
    .expect("the rule runs");

    assert_eq!(
        rows(&findings),
        [row(
            "",
            "app/shelf",
            None,
            &message("app/shelf", 2, ".type.ts files", "loan")
        )]
    );
}

#[test]
fn an_unknown_option_or_an_empty_separator_is_an_error() {
    assert!(error(RULE, r#"{ "separator": "-" }"#, &[]).contains("separator"));
    assert!(error(RULE, r#"{ "word-separator": "" }"#, &[]).contains("word-separator"));
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let files = tree(&[
        "src/dist/library/book-shelf/module.ts",
        "src/dist/library/book-cover/module.ts",
        "src/generated/library/book-shelf/module.ts",
        "src/generated/library/book-cover/module.ts",
    ]);

    assert_eq!(
        paths_with(RULE, "{}", SKIP_DIST, &files),
        ["src/dist/library", "src/generated/library"]
    );
}

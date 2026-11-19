//! `nestjs/repeated-segment` against synthetic source trees.

mod host;

use host::{Row, SKIP_DIST, check, error, paths_with, row, rows, run};

const RULE: &str = "repeated-segment";
const EMPTY: &str = "export {};\n";

fn tree(paths: &[&'static str]) -> Vec<(&'static str, &'static str)> {
    paths.iter().map(|path| (*path, EMPTY)).collect()
}

fn finding(path: &str, segment: &str) -> Row {
    row(
        "",
        path,
        None,
        &format!("{path} repeats parent folder segment \"{segment}\" in its filename"),
    )
}

#[test]
fn the_first_repeated_folder_segment_is_reported_once_per_file() {
    let cases = [
        ("src/shelf/utility/shelf-check.utility.ts", "shelf"),
        ("src/shelf/dto/shelf-check.dto.ts", "shelf"),
        ("src/shelf/factory/shelf-check.factory.ts", "shelf"),
        (
            "src/modules/domain/config/utility/config-reader.utility.ts",
            "config",
        ),
        ("src/modules/domain/shelf/type/type-reader.type.ts", "type"),
        (
            "src/modules/domain/shelf/interface/internal/internal-contract.interface.ts",
            "internal",
        ),
        (
            "src/modules/domain/shelf/domain/utility/domain-reader.utility.ts",
            "domain",
        ),
        ("src/shelf/src/utility/src-reader.utility.ts", "src"),
        (
            "src/modules/domain/shelf/migration/utility/migration-reader.utility.ts",
            "migration",
        ),
        (
            "src/shelf/book-loan/utility/read-book-loan-state.utility.ts",
            "book-loan",
        ),
        ("src/shelf/a/b/a-b-twice.utility.ts", "a"),
    ];
    let mut paths: Vec<&str> = cases.iter().map(|(path, _)| *path).collect();
    paths.sort_unstable();
    let mut expected: Vec<Row> = cases
        .iter()
        .map(|(path, segment)| finding(path, segment))
        .collect();
    expected.sort();

    assert_eq!(check(RULE, "{}", &tree(&paths)), expected);
}

#[test]
fn structural_segments_partial_words_and_files_without_a_kind_pass() {
    let files = tree(&[
        "src/shelf/interface/input.interface.ts",
        "src/shelf/type/type.ts",
        "src/shelf/utility/shelfless.utility.ts",
        "src/shelf/type/shelf.d.ts",
        "src/shelf/shelf/index.ts",
        "src/shelf/index/index.utility.ts",
        "src/shelf/shelf..ts",
        "src/config/utility/config-loader.utility.ts",
        "src/migration/config/constant/migration.constant.ts",
        "src/modules/domain/shelf/utility/domain-reader.utility.ts",
        "src/modules/internal/shelf/utility/internal-reader.utility.ts",
        "src/modules/public/shelf/utility/public-reader.utility.ts",
        "src/shelf/book-loan/utility/book-reader.utility.ts",
        "src/shelf/book-loan/utility/book-loanless.utility.ts",
        "src/shelf/notes/shelf-notes.md",
        "lib/shelf/shelf-check.utility.ts",
    ]);

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn ignored_stems_are_an_option() {
    let files = tree(&["src/shelf/index/index.utility.ts"]);

    assert_eq!(
        check(RULE, r#"{ "ignored-stems": [] }"#, &files),
        [finding("src/shelf/index/index.utility.ts", "index")]
    );
}

#[test]
fn the_layout_names_the_structural_folders() {
    let files = tree(&[
        "app/catalog/utility/catalog-reader.utility.ts",
        "app/features/edge/shelf/utility/edge-reader.utility.ts",
        "app/features/domain/shelf/utility/domain-reader.utility.ts",
        "app/shelf/utility/shelf-check.utility.ts",
        "src/shelf/utility/shelf-check.utility.ts",
    ]);
    let layout = r#"{ "layout": { "source-root": "app", "source-owners": ["catalog", "features"], "modules-root": "app/features", "module-planes": ["edge"] } }"#;
    let findings = run(RULE, "{}", layout, &files).expect("the rule runs");

    assert_eq!(
        rows(&findings),
        [
            finding(
                "app/features/domain/shelf/utility/domain-reader.utility.ts",
                "domain"
            ),
            finding("app/shelf/utility/shelf-check.utility.ts", "shelf"),
        ]
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "stems": [] }"#, &[]).contains("stems"));
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let files = tree(&[
        "src/dist/shelf/utility/shelf-check.utility.ts",
        "src/generated/shelf/dto/shelf-check.dto.ts",
    ]);

    assert_eq!(
        paths_with(RULE, "{}", SKIP_DIST, &files),
        [
            "src/dist/shelf/utility/shelf-check.utility.ts",
            "src/generated/shelf/dto/shelf-check.dto.ts"
        ]
    );
}

//! `nestjs/owner-roots` against synthetic source trees.

mod host;

use host::{Row, SKIP_DIST, check, error, paths_with, row, rows, run};

const RULE: &str = "owner-roots";
const OPTIONS: &str = r#"{
    "required-roots": [
        "src/modules/domain",
        "src/modules/integration",
        "src/modules/internal",
        "src/modules/public",
        "src/infrastructure"
    ],
    "retired-paths": ["src/shared/module"]
}"#;

fn missing(root: &str) -> Row {
    row(
        "owner-root",
        root,
        None,
        &format!("{root} must contain source owned by the accepted architecture boundary"),
    )
}

fn unknown(path: &str) -> Row {
    row(
        "owner-root",
        path,
        None,
        &format!("{path} is not an accepted architecture owner root"),
    )
}

fn retired(path: &str) -> Row {
    row(
        "shared-composition",
        path,
        None,
        &format!("{path} is a stale Nest composition owner and must be removed"),
    )
}

#[test]
fn roots_holding_any_ts_file_planes_only_and_no_retired_path_pass() {
    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    "src/modules/domain/book/entity.ts",
                    "export class Book {}\n"
                ),
                ("src/modules/integration/book/types.d.ts", "export {};\n"),
                (
                    "src/modules/internal/node_modules/x/index.ts",
                    "export {};\n"
                ),
                (
                    "src/modules/public/build/owner.ts",
                    "export class Owner {}\n"
                ),
                ("src/infrastructure/library/index.ts", "export {};\n"),
                ("src/shared/constant/index.ts", "export {};\n"),
                ("src/shared/module.ts", "export {};\n"),
            ]
        ),
        []
    );
}

#[test]
fn empty_roots_unknown_module_folders_and_a_retired_folder_are_reported() {
    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[
                (
                    "src/modules/domain/book/entity.ts",
                    "export class Book {}\n"
                ),
                ("src/modules/internal/README.md", "# internal\n"),
                ("src/infrastructure/library/script.js", "export {};\n"),
                ("src/modules/catalog/controller.ts", "export {};\n"),
                ("src/modules/archive/README.md", "# archive\n"),
                ("src/modules/loose.ts", "export {};\n"),
                ("src/shared/module/index.ts", "export {};\n"),
            ]
        ),
        [
            missing("src/modules/integration"),
            missing("src/modules/internal"),
            missing("src/modules/public"),
            missing("src/infrastructure"),
            unknown("src/modules/archive"),
            unknown("src/modules/catalog"),
            retired("src/shared/module"),
        ]
    );
}

#[test]
fn a_retired_file_is_reported() {
    assert_eq!(
        check(
            RULE,
            r#"{ "required-roots": [], "retired-paths": ["src/shared/module"] }"#,
            &[("src/shared/module", "stale\n")]
        ),
        [retired("src/shared/module")]
    );
}

#[test]
fn a_root_outside_the_source_root_is_never_satisfied() {
    assert_eq!(
        check(
            RULE,
            r#"{ "required-roots": ["lib/core"] }"#,
            &[("lib/core/index.ts", "export {};\n")]
        ),
        [missing("lib/core")]
    );
}

#[test]
fn the_layout_names_the_source_root_modules_root_and_planes() {
    let layout = r#"{ "layout": {
        "source-root": "app",
        "modules-root": "app/features",
        "module-planes": ["core", "edge"]
    } }"#;
    let files = [
        ("app/features/core/book.ts", "export {};\n"),
        ("app/features/edge/book.ts", "export {};\n"),
        ("app/features/domain/book.ts", "export {};\n"),
        ("src/modules/catalog/book.ts", "export {};\n"),
    ];

    assert_eq!(
        rows(
            &run(
                RULE,
                r#"{ "required-roots": ["app/features/core", "src/modules/catalog"] }"#,
                layout,
                &files
            )
            .expect("the rule runs")
        ),
        [
            missing("src/modules/catalog"),
            unknown("app/features/domain"),
        ]
    );
}

#[test]
fn the_required_roots_are_required() {
    assert!(error(RULE, "{}", &[]).contains("required-roots"));
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "required-roots": [], "roots": [] }"#, &[]).contains("roots"));
}

#[test]
fn files_in_skipped_folders_are_still_read() {
    let files = [
        ("src/modules/domain/dist/book.ts", "export {};\n"),
        ("src/modules/generated/x.ts", "export {};\n"),
    ];

    assert_eq!(
        paths_with(
            RULE,
            r#"{ "required-roots": ["src/modules/domain"] }"#,
            SKIP_DIST,
            &files
        ),
        ["src/modules/generated"]
    );
}

#[test]
fn the_project_root_is_refused_as_a_root_or_retired_path() {
    let required = error(RULE, r#"{ "required-roots": [""] }"#, &[]);
    let retired = error(
        RULE,
        r#"{ "required-roots": ["src"], "retired-paths": [""] }"#,
        &[],
    );

    assert!(
        required.contains("required-roots[0]") && required.contains("project root"),
        "{required}"
    );
    assert!(
        retired.contains("retired-paths[0]") && retired.contains("project root"),
        "{retired}"
    );
}

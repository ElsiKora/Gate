//! `nestjs/barrel-exports` against synthetic source trees.

mod support;

use support::{check, error, row, run};

const RULE: &str = "barrel-exports";
const PRIVATE: &str =
    r#"{ "private-entrypoints": ["src/runtime/api/main.ts", "src/runtime/api/main.ts"] }"#;

/// A tree whose barrels are complete.
fn clean() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/modules/index.ts", "export * from \"./user\";\n"),
        (
            "src/modules/user/index.ts",
            "export * from \"./dto\";\nexport { UserService } from './user.service';\n",
        ),
        (
            "src/modules/user/user.service.ts",
            "export class UserService {}\n",
        ),
        (
            "src/modules/user/dto/index.ts",
            "export { CreateDto } from \"./create.dto\";\n",
        ),
        (
            "src/modules/user/dto/create.dto.ts",
            "export class CreateDto {}\n",
        ),
        (
            "src/runtime/api/main.ts",
            "export async function start(): Promise<void> {}\n",
        ),
        ("src/modules/user/README.md", "notes\n"),
    ]
}

fn with(changes: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut files: Vec<_> = clean()
        .into_iter()
        .filter(|(path, _)| !changes.iter().any(|(changed, _)| changed == path))
        .collect();
    files.extend(changes.iter().filter(|(_, text)| !text.is_empty()).copied());
    files
}

#[test]
fn complete_barrels_and_listed_entrypoints_pass() {
    assert_eq!(check(RULE, PRIVATE, &clean()), []);
}

/// The clean tree with its one runtime started from the source root's `main.ts`, as a Nest app has
/// it.
fn nest_app() -> Vec<(&'static str, &'static str)> {
    with(&[
        ("src/runtime/api/main.ts", ""),
        (
            "src/main.ts",
            "async function bootstrap(): Promise<void> {}\nvoid bootstrap();\n",
        ),
    ])
}

#[test]
fn the_source_roots_main_ts_is_approved_when_the_option_is_unset() {
    assert_eq!(check(RULE, "{}", &nest_app()), []);
    // The default follows layout.source-root.
    let moved = [("app/main.ts", "void 0;\n")];
    let findings = run(
        RULE,
        "{}",
        r#"{ "layout": { "source-root": "app" } }"#,
        &moved,
    )
    .expect("the rule runs");
    assert_eq!(support::rows(&findings), []);
    // And entrypoint-suffix: the source root's file with that ending is the one approved.
    let server = with(&[
        ("src/runtime/api/main.ts", ""),
        ("src/server.ts", "void 0;\n"),
        ("src/runtime/worker/server.ts", "void 0;\n"),
    ]);
    assert_eq!(
        check(RULE, r#"{ "entrypoint-suffix": "/server.ts" }"#, &server),
        [
            row(
                "runtime-entrypoint",
                "src/runtime/worker/server.ts",
                None,
                "src/runtime/worker/server.ts is not an approved private runtime entrypoint"
            ),
            row(
                "missing",
                "src/runtime/index.ts",
                None,
                "src/runtime must expose an index.ts barrel"
            ),
            row(
                "missing",
                "src/runtime/worker/index.ts",
                None,
                "src/runtime/worker must expose an index.ts barrel"
            ),
        ]
    );
}

#[test]
fn a_list_replaces_the_default_and_the_help_names_the_rule_option() {
    let unapproved = [row(
        "runtime-entrypoint",
        "src/main.ts",
        None,
        "src/main.ts is not an approved private runtime entrypoint",
    )];
    assert_eq!(
        check(RULE, r#"{ "private-entrypoints": [] }"#, &nest_app()),
        unapproved
    );
    let listed = r#"{ "private-entrypoints": ["src/runtime/api/main.ts"] }"#;
    assert_eq!(check(RULE, listed, &nest_app()), unapproved);

    let findings = run(RULE, "{}", "{}", &clean()).expect("the rule runs");
    let help = findings
        .iter()
        .find(|finding| finding.aspect == "runtime-entrypoint")
        .and_then(|finding| finding.help.as_deref())
        .expect("a help");
    assert!(
        help.contains("the rule's option private-entrypoints")
            && help.contains("\"nestjs/barrel-exports\" = { options = { private-entrypoints"),
        "{help}"
    );
}

#[test]
fn every_entrypoint_is_reported_when_none_is_listed() {
    assert_eq!(
        check(RULE, "{}", &clean()),
        [
            row(
                "runtime-entrypoint",
                "src/runtime/api/main.ts",
                None,
                "src/runtime/api/main.ts is not an approved private runtime entrypoint"
            ),
            row(
                "missing",
                "src/runtime/index.ts",
                None,
                "src/runtime must expose an index.ts barrel"
            ),
            row(
                "missing",
                "src/runtime/api/index.ts",
                None,
                "src/runtime/api must expose an index.ts barrel"
            ),
        ]
    );
}

#[test]
fn a_folder_or_kind_folder_without_a_barrel_is_reported() {
    let files = with(&[("src/modules/user/dto/index.ts", "")]);

    assert_eq!(
        check(RULE, PRIVATE, &files),
        [
            row(
                "missing",
                "src/modules/user/dto/index.ts",
                None,
                "src/modules/user/dto must expose an index.ts barrel"
            ),
            row(
                "missing",
                "src/modules/user/dto/index.ts",
                None,
                "src/modules/user/dto is a kind folder and must expose an index.ts barrel"
            ),
            row(
                "wildcard",
                "src/modules/user/index.ts",
                Some(1),
                "src/modules/user/index.ts re-exports ./dto with a wildcard; use named file \
                 exports or a child-folder wildcard"
            ),
        ]
    );
}

#[test]
fn a_missing_export_is_reported_once_by_both_walks() {
    let files = with(&[(
        "src/modules/user/dto/update.dto.ts",
        "export class UpdateDto {}\n",
    )]);

    assert_eq!(
        check(RULE, PRIVATE, &files),
        [row(
            "explicit-export",
            "src/modules/user/dto/index.ts",
            None,
            "src/modules/user/dto/index.ts must export ./update.dto"
        )]
    );
}

#[test]
fn a_kind_folder_barrel_counts_only_double_quoted_specifiers() {
    let files = with(&[(
        "src/modules/user/dto/index.ts",
        "export { CreateDto } from './create.dto';\n",
    )]);

    assert_eq!(
        check(RULE, PRIVATE, &files),
        [row(
            "explicit-export",
            "src/modules/user/dto/index.ts",
            None,
            "src/modules/user/dto/index.ts must export ./create.dto"
        )]
    );
}

#[test]
fn a_barrel_with_code_is_reported_at_its_first_owned_statement() {
    let files = with(&[(
        "src/modules/user/index.ts",
        "export * from \"./dto\";\nexport { UserService } from \"./user.service\";\nexport const VERSION = 1;\n",
    )]);

    assert_eq!(
        check(RULE, PRIVATE, &files),
        [row(
            "reexport-only",
            "src/modules/user/index.ts",
            Some(3),
            "src/modules/user/index.ts must contain only re-export declarations; code owners \
             require their own files"
        )]
    );
}

#[test]
fn a_barrel_must_not_expose_a_private_entrypoint_or_a_foreign_wildcard() {
    let files = with(&[
        (
            "src/runtime/api/index.ts",
            "export { start } from \"./main\";\n",
        ),
        ("src/runtime/index.ts", "export * from \"./api\";\n"),
        (
            "src/modules/user/index.ts",
            "export * from \"./dto\";\nexport { UserService } from \"./user.service\";\nexport * from \"../shared\";\n",
        ),
    ]);

    assert_eq!(
        check(RULE, PRIVATE, &files),
        [
            row(
                "wildcard",
                "src/modules/user/index.ts",
                Some(3),
                "src/modules/user/index.ts re-exports ../shared with a wildcard; use named file \
                 exports or a child-folder wildcard"
            ),
            row(
                "private-entrypoint",
                "src/runtime/api/index.ts",
                Some(1),
                "src/runtime/api/index.ts must not expose the private ./main entrypoint"
            ),
        ]
    );
}

#[test]
fn the_barrel_name_and_entrypoint_suffix_are_options() {
    let options = r#"{ "barrel-name": "barrel.ts", "entrypoint-suffix": "/start.ts" }"#;
    let files = [
        ("src/app/barrel.ts", "export { a } from \"./a\";\n"),
        ("src/app/a.ts", "export const a = 1;\n"),
        ("src/app/b.ts", "export const b = 1;\n"),
        ("src/main.ts", "export const c = 1;\n"),
    ];

    assert_eq!(
        check(RULE, options, &files),
        [row(
            "explicit-export",
            "src/app/barrel.ts",
            None,
            "src/app/barrel.ts must export ./b"
        )]
    );

    let message = error(RULE, r#"{ "privateEntrypoints": [] }"#, &[]);
    assert!(message.contains("privateEntrypoints"), "{message}");
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let code = "export const VERSION = 1;\n";
    let generated_barrel = "src/modules/user/generated/index.ts";
    let vendored_barrel = "src/modules/user/node_modules/index.ts";
    let mut files = clean();
    files.extend([
        (generated_barrel, code),
        (vendored_barrel, code),
        ("src/shared/index.ts", "// shared\n"),
        ("src/shared/generated/gen.ts", "export const GEN = 1;\n"),
    ]);
    let found = |languages: &str| {
        support::rows(
            &support::run_with(RULE, PRIVATE, "{}", languages, &files).expect("the rule runs"),
        )
    };
    let code_row = |path: &str| {
        row(
            "reexport-only",
            path,
            Some(1),
            &format!(
                "{path} must contain only re-export declarations; code owners require their own \
                 files"
            ),
        )
    };

    assert_eq!(
        found("{}"),
        [
            row(
                "explicit-export",
                "src/shared/index.ts",
                None,
                "src/shared/index.ts must export ./generated"
            ),
            row(
                "missing",
                "src/shared/generated/index.ts",
                None,
                "src/shared/generated must expose an index.ts barrel"
            ),
            code_row(generated_barrel),
        ]
    );
    assert_eq!(found(support::SKIP_GENERATED), [code_row(vendored_barrel)]);
}

#[test]
fn a_folder_whose_only_typescript_file_is_skipped_needs_no_barrel() {
    let files = with(&[
        ("src/modules/user/assets/logo.svg", "<svg/>\n"),
        (
            "src/modules/user/assets/generated/icon.ts",
            "export const ICON = 1;\n",
        ),
    ]);
    let assets = |languages: &str| -> Vec<support::Row> {
        support::rows(
            &support::run_with(RULE, PRIVATE, "{}", languages, &files).expect("the rule runs"),
        )
        .into_iter()
        .filter(|(_, path, _, message)| path.contains("/assets") || message.contains("./assets"))
        .collect()
    };

    assert!(
        assets("{}").contains(&row(
            "missing",
            "src/modules/user/assets/index.ts",
            None,
            "src/modules/user/assets must expose an index.ts barrel"
        )),
        "{:?}",
        assets("{}")
    );
    assert_eq!(assets(support::SKIP_GENERATED), []);
}

#[test]
fn a_wildcard_into_a_child_folder_of_skipped_files_names_no_child() {
    let files = with(&[
        (
            "src/modules/user/index.ts",
            "export * from \"./dto\";\nexport { UserService } from './user.service';\nexport * from \"./generated\";\n",
        ),
        (
            "src/modules/user/generated/index.ts",
            "export { GEN } from \"./gen\";\n",
        ),
        (
            "src/modules/user/generated/gen.ts",
            "export const GEN = 1;\n",
        ),
    ]);
    let wildcards = |languages: &str| -> Vec<support::Row> {
        support::rows(
            &support::run_with(RULE, PRIVATE, "{}", languages, &files).expect("the rule runs"),
        )
        .into_iter()
        .filter(|(aspect, _, _, _)| aspect == "wildcard")
        .collect()
    };

    assert_eq!(wildcards("{}"), []);
    assert_eq!(
        wildcards(support::SKIP_GENERATED),
        [row(
            "wildcard",
            "src/modules/user/index.ts",
            Some(3),
            "src/modules/user/index.ts re-exports ./generated with a wildcard; use named file \
             exports or a child-folder wildcard"
        )]
    );
}

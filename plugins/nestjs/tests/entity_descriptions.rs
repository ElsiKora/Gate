//! `nestjs/entity-descriptions` against synthetic entities.

mod host;

use host::{SKIP_GENERATED, error, paths_with, row, rows, run, targets};

const RULE: &str = "entity-descriptions";
const ENTITY: &str = "src/modules/shelf/shelf.entity.ts";

fn entity(members: &str) -> String {
    format!(
        "import {{ ApiPropertyDescribe }} from \"@elsikora/nestjs-crud-automator\";\n\
         export class LibraryShelfEntity {{\n{members}}}\n"
    )
}

#[test]
fn lowercase_static_descriptions_pass() {
    let members = "  @ApiPropertyDescribe({ description: \"position on the wall\" })\n\
                   \x20 position!: number;\n\
                   \x20 @ApiPropertyDescribe({ \"description\": `label` })\n\
                   \x20 label!: string;\n\
                   \x20 @Other({ description: \"Shelf thing\" })\n\
                   \x20 other!: string;\n";

    assert_eq!(
        rows(&run(RULE, "{}", "{}", &[(ENTITY, &entity(members))]).expect("runs")),
        []
    );
}

#[test]
fn owner_prefixes_and_uppercase_tokens_are_reported_with_targets() {
    let members = "  @ApiPropertyDescribe({ description: \"shelf height\" })\n\
                   \x20 height!: number;\n\
                   \x20 @ApiPropertyDescribe({ description: \"Library shelf code\" })\n\
                   \x20 \"code\"!: string;\n\
                   \x20 @ApiPropertyDescribe({ description: \"TLS digest\" })\n\
                   \x20 digest!: string;\n";
    let findings = run(RULE, "{}", "{}", &[(ENTITY, &entity(members))]).expect("runs");

    assert_eq!(
        rows(&findings),
        [
            row(
                "owner-prefix",
                ENTITY,
                Some(3),
                "LibraryShelfEntity.height description repeats its owner/resource prefix: \"shelf height\""
            ),
            row(
                "owner-prefix",
                ENTITY,
                Some(5),
                "LibraryShelfEntity.code description repeats its owner/resource prefix: \"Library shelf code\""
            ),
            row(
                "uppercase-token",
                ENTITY,
                Some(5),
                "LibraryShelfEntity.code description starts with unjustified uppercase token \"Library\""
            ),
            row(
                "uppercase-token",
                ENTITY,
                Some(7),
                "LibraryShelfEntity.digest description starts with unjustified uppercase token \"TLS\""
            ),
        ]
    );
    assert_eq!(
        targets(&findings),
        [
            Some("LibraryShelfEntity.height: shelf height".to_owned()),
            Some("LibraryShelfEntity.code: Library shelf code".to_owned()),
            Some("LibraryShelfEntity.code: Library shelf code".to_owned()),
            Some("LibraryShelfEntity.digest: TLS digest".to_owned()),
        ]
    );
}

#[test]
fn descriptions_read_through_constants_and_dynamic_ones_are_reported() {
    let constant = "export const NOTE = \"note on the spine\";\n";
    let members = "  @ApiPropertyDescribe({ description: NOTE })\n\
                   \x20 note!: string;\n\
                   \x20 @ApiPropertyDescribe({ description: make() })\n\
                   \x20 made!: string;\n";
    let text = format!(
        "import {{ NOTE }} from \"./note.constant\";\n{}",
        entity(members)
    );

    assert_eq!(
        rows(
            &run(
                RULE,
                "{}",
                "{}",
                &[
                    (ENTITY, &text),
                    ("src/modules/shelf/note.constant.ts", constant)
                ]
            )
            .expect("runs")
        ),
        [row(
            "static",
            ENTITY,
            Some(6),
            "LibraryShelfEntity.made description must resolve to a static string"
        )]
    );
}

#[test]
fn options_allow_tokens_and_narrow_the_universe() {
    let members = "  @ApiPropertyDescribe({ description: \"TLS digest\" })\n\
                   \x20 digest!: string;\n";
    let text = entity(members);

    assert_eq!(
        rows(
            &run(
                RULE,
                r#"{ "allowed-initial-uppercase": ["TLS"] }"#,
                "{}",
                &[(ENTITY, &text)]
            )
            .expect("runs")
        ),
        []
    );
    assert_eq!(
        rows(
            &run(
                RULE,
                "{}",
                "{}",
                &[
                    ("lib/shelf.entity.ts", &text),
                    ("src/node_modules/shelf.entity.ts", &text),
                ]
            )
            .expect("runs")
        ),
        []
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "allowed": [] }"#, &[]).contains("allowed"));
}

#[test]
fn the_language_skip_list_decides_which_folders_are_read() {
    let text = entity(
        "  @ApiPropertyDescribe({ description: \"shelf height\" })\n\x20 height!: number;\n",
    );
    let files = [
        ("src/generated/shelf.entity.ts", text.as_str()),
        ("src/modules/build/shelf.entity.ts", text.as_str()),
        ("src/node_modules/shelf.entity.ts", text.as_str()),
    ];

    assert_eq!(
        paths_with(RULE, "{}", "", &files),
        [
            "src/generated/shelf.entity.ts",
            "src/modules/build/shelf.entity.ts"
        ]
    );
    assert_eq!(
        paths_with(RULE, "{}", SKIP_GENERATED, &files),
        [
            "src/modules/build/shelf.entity.ts",
            "src/node_modules/shelf.entity.ts"
        ]
    );
}

#[test]
fn there_is_no_skip_prefix_option() {
    assert!(error(RULE, r#"{ "skip-prefixes": [] }"#, &[]).contains("skip-prefixes"));
}

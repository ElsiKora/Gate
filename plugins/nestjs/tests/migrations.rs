//! `nestjs/migrations` against synthetic migration trees.

mod host;

use host::{SKIP_GENERATED, check, error, paths_with, row};

const RULE: &str = "migrations";
const GENERATED: &str = "src/migration/typeorm/generated";
const BARREL: &str = "src/migration/typeorm/generated/index.ts";
const REGISTRY: &str = "src/migration/typeorm/migrations.ts";
const SHELVES: &str = "src/migration/typeorm/generated/1700000000001-CreateShelf.ts";
const BOOKS: &str = "src/migration/typeorm/generated/1700000000002-SeedBook.ts";

/// A fresh-only migration class.
fn migration(class: &str) -> String {
    format!(
        "import type {{ QueryRunner }} from \"typeorm\";\n\nexport class {class} {{\n name: string = \"{class}\";\n\n public async up(queryRunner: QueryRunner): Promise<void> {{\n  await queryRunner.query(\"SELECT 1 -- slow down\");\n }}\n}}\n"
    )
}

/// A barrel line.
fn export(class: &str, stem: &str) -> String {
    format!("export {{ {class} }} from \"./{stem}\";\n")
}

/// A registry that imports `imported` and lists `listed`.
fn registry(imported: &[&str], listed: &[&str]) -> String {
    format!(
        "import type {{ DataSourceOptions }} from \"typeorm\";\n\nimport {{\n {}\n}} from \"./generated\";\n\nexport const TYPEORM_MIGRATIONS_CONSTANT: Extract<\n NonNullable<DataSourceOptions[\"migrations\"]>,\n Array<unknown>\n> = [\n {}\n];\n",
        imported.join(",\n "),
        listed.join(",\n ")
    )
}

const SHELF: &str = "CreateShelf1700000000001";
const BOOK: &str = "SeedBook1700000000002";

/// Two valid migrations, their barrel and their registry.
fn valid() -> Vec<(String, String)> {
    vec![
        (SHELVES.to_owned(), migration(SHELF)),
        (BOOKS.to_owned(), migration(BOOK)),
        (
            BARREL.to_owned(),
            export(SHELF, "1700000000001-CreateShelf") + &export(BOOK, "1700000000002-SeedBook"),
        ),
        (
            REGISTRY.to_owned(),
            registry(&[SHELF, BOOK], &[SHELF, BOOK]),
        ),
    ]
}

fn borrowed(files: &[(String, String)]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect()
}

fn replace(files: &mut [(String, String)], path: &str, text: String) {
    for file in files.iter_mut() {
        if file.0 == path {
            file.1 = text;
            return;
        }
    }
    panic!("{path} is in the fixture");
}

#[test]
fn registered_fresh_only_migrations_pass_and_a_tree_without_migrations_is_silent() {
    assert_eq!(check(RULE, "{}", &borrowed(&valid())), []);
    assert_eq!(
        check(
            RULE,
            r#"{ "single-baseline": true }"#,
            &[("src/main.ts", "")]
        ),
        []
    );
}

#[test]
fn class_contract_breaches_are_reported_at_the_class() {
    let mut files = valid();
    replace(
        &mut files,
        SHELVES,
        format!(
            "export class {SHELF} implements Contract {{\n name: string = \"CreateShelf\";\n down(): void {{}}\n}}\n"
        ),
    );
    replace(
        &mut files,
        BOOKS,
        "export class OtherName1700000000002 {}\n".to_owned(),
    );
    let shelves = "1700000000001-CreateShelf.ts";

    assert_eq!(
        check(RULE, "{}", &borrowed(&files)),
        [
            row(
                "class",
                SHELVES,
                Some(1),
                &format!(
                    "{shelves} must declare {SHELF} as a plain class without extends, implements, abstract or declare"
                )
            ),
            row(
                "class",
                SHELVES,
                Some(1),
                &format!("{shelves} must not expose an executable downgrade contract")
            ),
            row(
                "class",
                SHELVES,
                Some(1),
                &format!("{shelves} must set name: string = \"{SHELF}\"")
            ),
            row(
                "class",
                BOOKS,
                None,
                &format!("1700000000002-SeedBook.ts must export the fresh-only class {BOOK}")
            ),
        ]
    );
}

#[test]
fn abstract_declared_and_extending_classes_are_not_plain() {
    for text in [
        format!("export abstract class {SHELF} {{\n name: string = \"{SHELF}\";\n}}\n"),
        format!("export declare class {SHELF} {{\n name: string;\n}}\n"),
        format!("export class {SHELF} extends Base {{\n name: string = \"{SHELF}\";\n}}\n"),
    ] {
        let mut files = valid();
        replace(&mut files, SHELVES, text);
        let rows = check(RULE, "{}", &borrowed(&files));

        assert!(
            rows.iter().any(|row| row.3.contains("as a plain class")),
            "{rows:?}"
        );
    }
}

#[test]
fn bad_names_and_duplicate_timestamps_are_reported() {
    let again = "src/migration/typeorm/generated/1700000000001-CreateShelfAgain.ts";
    let short = "src/migration/typeorm/generated/170000000003-AddAuthor.ts";
    let mut files = valid();
    files.push((again.to_owned(), migration("CreateShelfAgain1700000000001")));
    files.push((short.to_owned(), migration("AddAuthor170000000003")));
    replace(
        &mut files,
        BARREL,
        export(SHELF, "1700000000001-CreateShelf")
            + &export(
                "CreateShelfAgain1700000000001",
                "1700000000001-CreateShelfAgain",
            )
            + &export(BOOK, "1700000000002-SeedBook"),
    );
    replace(
        &mut files,
        REGISTRY,
        registry(
            &[SHELF, "CreateShelfAgain1700000000001", BOOK],
            &[SHELF, "CreateShelfAgain1700000000001", BOOK],
        ),
    );

    assert_eq!(
        check(RULE, "{}", &borrowed(&files)),
        [
            row(
                "timestamp",
                again,
                None,
                "Duplicate migration timestamp \"1700000000001\""
            ),
            row(
                "filename",
                short,
                None,
                "Invalid migration filename \"170000000003-AddAuthor.ts\"; use \"<timestamp>-<SemanticName>.ts\""
            ),
        ]
    );
}

#[test]
fn a_stale_barrel_and_registry_order_are_reported() {
    let mut files = valid();
    replace(
        &mut files,
        BARREL,
        export(BOOK, "1700000000002-SeedBook") + &export(SHELF, "1700000000001-CreateShelf"),
    );
    replace(&mut files, REGISTRY, registry(&[SHELF], &[BOOK, SHELF]));

    assert_eq!(
        check(RULE, "{}", &borrowed(&files)),
        [
            row(
                "barrel",
                BARREL,
                None,
                "The generated migration barrel is not timestamp ordered or does not match the generated migration files"
            ),
            row(
                "registry",
                REGISTRY,
                None,
                "The migration registry imports do not match the generated migrations"
            ),
            row(
                "registry",
                REGISTRY,
                Some(10),
                "TYPEORM_MIGRATIONS_CONSTANT does not match the timestamp-ordered generated migrations"
            ),
        ]
    );
}

#[test]
fn type_imports_missing_constants_and_missing_files_are_reported() {
    let mut files = valid();
    replace(
        &mut files,
        REGISTRY,
        format!(
            "import {{ type {SHELF} }} from \"./generated\";\nimport type {{ {BOOK} }} from \"./generated\";\n"
        ),
    );

    assert_eq!(
        check(RULE, "{}", &borrowed(&files)),
        [row(
            "registry",
            REGISTRY,
            None,
            "The migration registry must import the migrations explicitly from ./generated"
        )]
    );

    replace(
        &mut files,
        REGISTRY,
        format!(
            "import {{ {SHELF}, {BOOK} }} from \"./generated\";\nexport const MIGRATIONS = [{SHELF}, {BOOK}];\n"
        ),
    );

    assert_eq!(
        check(RULE, "{}", &borrowed(&files)),
        [row(
            "registry",
            REGISTRY,
            None,
            "The migration registry must declare TYPEORM_MIGRATIONS_CONSTANT as an explicit array"
        )]
    );

    let barrel = export(SHELF, "1700000000001-CreateShelf");

    assert_eq!(
        check(RULE, "{}", &[(BARREL, barrel.as_str())]),
        [
            row(
                "barrel",
                BARREL,
                None,
                "The generated migration barrel is not timestamp ordered or does not match the generated migration files"
            ),
            row(
                "registry",
                REGISTRY,
                None,
                "The TypeORM migration registry is missing"
            ),
        ]
    );

    let shelf = migration(SHELF);

    assert_eq!(
        check(
            RULE,
            "{}",
            &[(SHELVES, shelf.as_str()), (REGISTRY, "class {")]
        ),
        [row(
            "barrel",
            BARREL,
            None,
            "The generated migration barrel is missing"
        )]
    );
}

#[test]
fn a_single_baseline_keeps_one_migration() {
    let options = r#"{ "single-baseline": true }"#;

    assert_eq!(
        check(RULE, options, &borrowed(&valid())),
        [row(
            "baseline",
            BOOKS,
            None,
            &format!(
                "A pre-launch product keeps exactly one generated migration; fold {BOOK} into the baseline {SHELF} instead of adding a migration"
            )
        )]
    );

    let empty = registry(&[], &[]).replace("import {\n \n} from \"./generated\";\n\n", "");

    assert_eq!(
        check(RULE, options, &[(BARREL, ""), (REGISTRY, empty.as_str())]),
        [
            row(
                "baseline",
                GENERATED,
                None,
                "A pre-launch product keeps exactly one generated migration, its baseline, and has none"
            ),
            row(
                "registry",
                REGISTRY,
                None,
                "The migration registry must import the migrations explicitly from ./generated"
            ),
        ]
    );
}

#[test]
fn options_move_the_files_and_rename_the_constant() {
    let options = r#"{
        "generated-folder": "db/generated",
        "barrel": "db/generated/all.ts",
        "registry": "db/list.ts",
        "registry-constant": "MIGRATIONS",
        "registry-import": "./generated/all",
        "timestamp-digits": 4
    }"#;
    let class = "AddShelf1234";
    let text = migration(class);
    let barrel = export(class, "1234-AddShelf");
    let list = format!(
        "import {{ {class} }} from \"./generated/all\";\nexport const MIGRATIONS = [{class}];\n"
    );
    let files = [
        ("db/generated/1234-AddShelf.ts", text.as_str()),
        ("db/generated/all.ts", barrel.as_str()),
        ("db/list.ts", list.as_str()),
    ];

    assert_eq!(check(RULE, options, &files), []);
    assert_eq!(
        check(RULE, "{}", &files),
        [] as [host::Row; 0],
        "the default paths see no migration"
    );

    let wrong = list.replace("[AddShelf1234]", "[]");

    assert_eq!(
        check(
            RULE,
            options,
            &[
                ("db/generated/1234-AddShelf.ts", text.as_str()),
                ("db/generated/all.ts", barrel.as_str()),
                ("db/list.ts", wrong.as_str()),
            ]
        ),
        [row(
            "registry",
            "db/list.ts",
            Some(2),
            "MIGRATIONS does not match the timestamp-ordered generated migrations"
        )]
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "preLaunch": true }"#, &[]).contains("preLaunch"));
}

#[test]
fn a_barrel_that_is_not_utf8_is_a_barrel_finding() {
    let files = valid();
    let fixture = host::Fixture::new(&borrowed(&files)).with(BARREL, &[0xff, 0xfe, b'\n']);
    let found = host::run_on(RULE, &host::Request::default(), &fixture).expect("the rule runs");

    assert_eq!(
        host::rows(&found),
        [row(
            "barrel",
            BARREL,
            None,
            "The generated migration barrel is not timestamp ordered or does not match the generated migration files"
        )]
    );
}

#[test]
fn a_path_option_out_of_normal_form_is_an_error_showing_the_normal_form() {
    let message = error(
        RULE,
        r#"{ "generated-folder": "./src/migration//generated/" }"#,
        &[],
    );

    assert!(message.contains("\"src/migration/generated\""), "{message}");
    assert!(error(RULE, r#"{ "barrel": "../index.ts" }"#, &[]).contains("leaves the project"));
}

#[test]
fn migrations_in_a_folder_the_language_list_skips_are_still_read() {
    let mut files = valid();
    replace(
        &mut files,
        BOOKS,
        "export class OtherName1700000000002 {}\n".to_owned(),
    );

    // `generated-folder` is named `generated`, which this language list skips.
    assert_eq!(
        paths_with(RULE, "{}", SKIP_GENERATED, &borrowed(&files)),
        [BOOKS]
    );
}

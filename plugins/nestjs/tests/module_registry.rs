//! `nestjs/module-registry` against synthetic modules, registries and chapters.

mod host;

use host::{Request, SKIP_GENERATED, check, error, row, rows, run, run_with};

const RULE: &str = "module-registry";
const REGISTRY: &str = "docs/modules/module-registry.md";
const HEADER: &str = "| Module Path | Chapter | Ownership |\n| --- | --- | --- |\n";

/// A native module class in a file.
fn module(class: &str) -> String {
    format!(
        "import {{ Module }} from \"@nestjs/common\";\n\n@Module({{}})\nexport class {class} {{}}\n"
    )
}

/// A registry row.
fn entry(path: &str, chapter: &str) -> String {
    format!("| `{path}` | [Owner]({chapter}) | Library team |\n")
}

/// The rows of every aspect but `owner-chapter`, which its own tests cover.
fn registry_rows(rows: Vec<host::Row>) -> Vec<host::Row> {
    rows.into_iter()
        .filter(|row| row.0 != "owner-chapter")
        .collect()
}

fn shape(line: Option<u32>, message: &str) -> host::Row {
    row("registry-shape", REGISTRY, line, message)
}

#[test]
fn every_native_module_mapped_by_its_chapter_passes() {
    let registry = format!(
        "# Modules\n\n```md\n{HEADER}| `src/example/module.ts` | [E](api/example.md) | E |\n```\n\n<!--\n{HEADER}| `src/hidden/module.ts` | [H](api/hidden.md) | H |\n-->\n\n| Module Path | Chapter | Ownership |\n| :--- | :---: | ---: |\n{}{}{}{}{}{}",
        entry("src/book/module.ts", "api/book.md#source-map"),
        entry("src/author/feature.ts", "./shelf/../api/book.md"),
        entry("src/library/owner.ts", "../modules/api/library.md"),
        entry("src/loan/optional.ts", "api/library.md"),
        entry("src/shelf/member.ts", "api/library.md"),
        entry("src/reader/asserted.ts", "api/library.md"),
    );
    let book = "## Purpose\n\n`src/library/owner.ts`\n\n## Source Map\n\n- `src/book/*.ts`\n- `src/author/feature.ts`\n\n### Detail\n\n## Links\n";
    let library = "## Source Map\t\n`src/*/owner.ts`\n`src/loan/*`\n`src/shelf/member.ts`\n`src/reader/asserted.ts`\n";
    let files = [
        ("src/book/module.ts", module("BookModule")),
        (
            "src/author/feature.ts",
            "import * as Nest from \"@nestjs/common\";\nconst Namespace = Nest;\nconst Decorate = (Namespace[\"Module\"]!);\n\n@Decorate({})\nexport class AuthorFeature {}\n".to_owned(),
        ),
        (
            "src/library/owner.ts",
            "import { Module as NativeModule } from \"@nestjs/common\";\n\nexport const Owner = @(NativeModule({}) as ClassDecorator) class {};\n".to_owned(),
        ),
        (
            "src/loan/optional.ts",
            "import { Module } from \"@nestjs/common\";\n\n@(Module?.({}))\nexport class Loan {}\n".to_owned(),
        ),
        (
            "src/shelf/member.ts",
            "import * as nest from \"@nestjs/common\";\n\n@(nest?.Module({}))\nexport class Shelf {}\n".to_owned(),
        ),
        (
            "src/reader/asserted.ts",
            "import * as nest from \"@nestjs/common\";\n\n@(nest?.Module!({})!)\nexport class Reader {}\n".to_owned(),
        ),
        (
            "src/base/abstract.ts",
            "import { Module } from \"@nestjs/common\";\n\n@Module({})\nexport abstract class Base {}\n".to_owned(),
        ),
        (
            "src/local/shadow.ts",
            "function Module(_: object): ClassDecorator {\n return () => undefined;\n}\n\n@Module({})\nexport class Shadow {}\n".to_owned(),
        ),
        (
            "src/core/foreign.ts",
            "import { Module } from \"@nestjs/core\";\n\n@Module({})\nexport class Foreign {}\n".to_owned(),
        ),
        ("src/types/types.d.ts", module("Declared")),
        ("src/node_modules/vendor/module.ts", module("Vendored")),
        ("src/.git/module.ts", module("Hidden")),
        (
            "src/plain/plain.ts",
            "import { Module } from \"@nestjs/common\";\n\nexport const options = Module;\n".to_owned(),
        ),
        ("src/broken/module.ts", format!("{}\nclass {{", module("Broken"))),
        (REGISTRY, registry),
        ("docs/modules/api/book.md", book.to_owned()),
        ("docs/modules/api/library.md", library.to_owned()),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn rows_chapters_and_coverage_are_reported_at_their_registry_lines() {
    let registry = format!(
        "# Modules\n\n{HEADER}{}{}{}{}{}{}| src/x/module.ts | [X](api/a.md) | X |\n| `src/y/module.ts` | [Y] (api/a.md) | Y |\n| `src/z/../module.ts` | [Z](api/a.md) | Z |\n| `src/w/module.ts` | | W |\n| `src/v/module.ts` | [V](api/a.md) |\n\n| Module Path | Chapter | Ownership |\n| -- | --- | --- |\n{}",
        entry("src/book/module.ts", "api/a.md"),
        entry("src/book/module.ts", "api/a.md"),
        entry("src/author/module.ts", "README.md"),
        entry("src/shelf/module.ts", "api/missing.md"),
        entry("src/loan/module.ts", "api/a.md"),
        entry("src/stale/module.ts", "api/a.md"),
        entry("src/reader/module.ts", "api/a.md"),
    );
    let chapter = "## Source Map\n\n- `src/book/module.ts`\n\n## Links\n\n- `src/loan/module.ts`\n";
    let mut files: Vec<(String, String)> = ["book", "author", "shelf", "loan", "reader"]
        .iter()
        .map(|name| (format!("src/{name}/module.ts"), module("Owner")))
        .collect();
    files.push((REGISTRY.to_owned(), registry));
    files.push(("docs/modules/api/a.md".to_owned(), chapter.to_owned()));
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let chapter = |line: u32, message: &str| {
        row(
            "registry-chapter",
            REGISTRY,
            Some(line),
            &format!("{REGISTRY}:{line} {message}"),
        )
    };
    let malformed = |line: u32, what: &str| {
        shape(
            Some(line),
            &format!("{REGISTRY}:{line} has a malformed module {what}"),
        )
    };

    assert_eq!(
        registry_rows(check(RULE, "{}", &files)),
        [
            shape(
                Some(6),
                &format!("{REGISTRY} contains duplicate module path: src/book/module.ts")
            ),
            chapter(
                7,
                "must link a substantive module chapter: src/author/module.ts"
            ),
            chapter(8, "links missing module chapter: api/missing.md"),
            chapter(9, "chapter does not map source module: src/loan/module.ts"),
            malformed(11, "row"),
            malformed(12, "chapter link"),
            malformed(13, "row"),
            malformed(14, "row"),
            malformed(15, "row"),
            malformed(18, "table delimiter"),
            row(
                "registry-coverage",
                REGISTRY,
                None,
                &format!("{REGISTRY} is missing source module: src/reader/module.ts")
            ),
            row(
                "registry-coverage",
                REGISTRY,
                Some(10),
                &format!("{REGISTRY} references unknown source module: src/stale/module.ts")
            ),
            row(
                "registry-coverage",
                REGISTRY,
                Some(12),
                &format!("{REGISTRY} references unknown source module: src/y/module.ts")
            ),
        ]
    );
}

#[test]
fn a_missing_registry_and_a_registry_without_a_table_are_reported() {
    let book = module("BookModule");

    assert_eq!(
        check(RULE, "{}", &[("src/book/module.ts", book.as_str())]),
        [shape(
            None,
            &format!("{REGISTRY} is missing the module registry")
        )]
    );

    let fenced = format!(
        "# Modules\n\n~~~\n{HEADER}{}~~~\n",
        entry("src/book/module.ts", "api/book.md")
    );

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                ("src/book/module.ts", book.as_str()),
                (REGISTRY, fenced.as_str()),
                (
                    "docs/modules/api/book.md",
                    "## Source Map\n\n- `src/book/module.ts`\n"
                ),
            ]
        ),
        [
            shape(
                None,
                &format!("{REGISTRY} is missing the Module Path / Chapter / Ownership table")
            ),
            row(
                "registry-coverage",
                REGISTRY,
                None,
                &format!("{REGISTRY} is missing source module: src/book/module.ts")
            ),
        ]
    );
}

#[test]
fn the_source_map_section_ends_at_the_next_level_two_heading_and_reads_every_line_terminator() {
    let registry = format!("{HEADER}{}", entry("src/book/module.ts", "api/book.md"));
    let book = module("BookModule");
    let outcome = |chapter: &str| {
        check(
            RULE,
            "{}",
            &[
                ("src/book/module.ts", book.as_str()),
                (REGISTRY, registry.as_str()),
                ("docs/modules/api/book.md", chapter),
            ],
        )
        .len()
    };

    assert_eq!(outcome("## Source Map \n- `src/*/*.ts`\n"), 0);
    assert_eq!(
        outcome("## Source Map\n### Detail\n`src/book/module.ts`\n## Next\n"),
        0
    );
    assert_eq!(outcome("## Source Map\r`src/book/module.ts`\r## Next\r"), 0);
    assert_eq!(outcome("## Source Map\n## Next\n`src/book/module.ts`\n"), 1);
    assert_eq!(outcome("## Source Maps\n`src/book/module.ts`\n"), 1);
    assert_eq!(outcome("## Source Map\n`src/`src/book/module.ts`\n"), 0);
    assert_eq!(outcome("## Source Map\n`src/book/*.js`\n"), 1);
}

#[test]
fn cells_and_delimiters_read_as_their_patterns() {
    let book = module("BookModule");
    let outcome = |table: &str| {
        rows(
            &run(
                RULE,
                "{}",
                "{}",
                &[
                    ("src/book/module.ts", book.as_str()),
                    (REGISTRY, table),
                    (
                        "docs/modules/api/book.md",
                        "## Source Map\n`src/book/module.ts`\n",
                    ),
                ],
            )
            .expect("the rule runs"),
        )
        .into_iter()
        .filter(|row| row.0 != "owner-chapter")
        .map(|row| row.3.replace(REGISTRY, "R"))
        .collect::<Vec<_>>()
    };
    let row = "| `src/book/module.ts` | [Book](api/book.md) | Library |\n";
    let header = "| Module Path | Chapter | Ownership |\n";

    assert_eq!(
        outcome(&format!("{header}|---|:---:| --- |\n{row}")),
        [] as [String; 0]
    );
    assert_eq!(
        outcome(&format!("   {header}   | --- | --- | --- |\t\n   {row}")),
        [] as [String; 0]
    );
    assert_eq!(
        outcome(&format!("{header}    | --- | --- | --- |\n{row}")),
        [
            "R:2 has a malformed module table delimiter",
            "R is missing the Module Path / Chapter / Ownership table",
            "R is missing source module: src/book/module.ts",
        ]
    );
    assert_eq!(
        outcome(&format!(
            "{header}| --- | --- | --- |\n| `src/book/module.ts` | [](api/book.md) | L |\n"
        )),
        ["R:3 has a malformed module chapter link",]
    );
    assert_eq!(
        outcome(&format!(
            "{header}| --- | --- | --- |\n| `src/book/module.ts` | [Book](#source-map) | L |\n"
        )),
        ["R:3 has a malformed module chapter link"]
    );
    assert_eq!(
        outcome(&format!(
            "{header}| --- | --- | --- |\n| `src/book module.ts` | [Book](api/book.md) | L |\n"
        )),
        [
            "R:3 has a malformed module row",
            "R is missing source module: src/book/module.ts",
        ]
    );
    assert_eq!(
        outcome(&format!(
            "{header}| --- | --- | --- |\n| `src/book/module.ts` | [Book](../../other/book.md) | L |\n"
        )),
        [] as [String; 0],
        "a chapter outside the chapter root is not read"
    );
}

#[test]
fn options_rename_the_registry_its_header_folders_and_decorator() {
    let options = r####"{
        "registry": "guide/registry.md",
        "chapter-root": "guide",
        "chapter-index": "guide/index.md",
        "table-header": ["Path", "Page", "Team"],
        "source-map-heading": "### Files",
        "module-package": "@scope/web",
        "module-decorator": "Unit"
    }"####;
    let unit = "import { Unit } from \"@scope/web\";\n\n@Unit({})\nexport class BookUnit {}\n";
    let registry = "| Path | Page | Team |\n| --- | --- | --- |\n| `lib/book/unit.ts` | [Book](book.md) | Library |\n| `lib/book/unit.ts` | [Index](index.md) | Library |\n";
    let files = [
        ("lib/book/unit.ts", unit),
        ("lib/vendor/unit.ts", unit),
        (
            "lib/author/module.ts",
            "import { Module } from \"@nestjs/common\";\n\n@Module({})\nexport class AuthorModule {}\n",
        ),
        ("guide/registry.md", registry),
        ("guide/book.md", "### Files\n`lib/book/unit.ts`\n### Next\n"),
    ];
    let layout = r#"{ "layout": { "source-root": "lib" } }"#;
    let findings = registry_rows(rows(
        &run_with(
            RULE,
            &Request {
                options,
                plugin_options: layout,
                languages: r#"{ "typescript": { "skip-folders": ["vendor"] } }"#,
                ..Request::default()
            },
            &files,
        )
        .expect("the rule runs"),
    ));

    assert_eq!(
        findings,
        [
            row(
                "registry-shape",
                "guide/registry.md",
                Some(4),
                "guide/registry.md contains duplicate module path: lib/book/unit.ts"
            ),
            row(
                "registry-chapter",
                "guide/registry.md",
                Some(4),
                "guide/registry.md:4 must link a substantive module chapter: lib/book/unit.ts"
            ),
        ]
    );
}

#[test]
fn unknown_options_and_a_header_of_the_wrong_size_are_errors() {
    assert!(error(RULE, r#"{ "registry-path": "x.md" }"#, &[]).contains("registry-path"));
    assert!(error(RULE, r#"{ "table-header": ["Path", "Page"] }"#, &[]).contains("table-header"));
}

/// An `owner-chapter` finding of the registry.
fn unlinked(line: u32, owner: &str) -> host::Row {
    row(
        "owner-chapter",
        REGISTRY,
        Some(line),
        &format!("{REGISTRY} {owner} must link one local documentation chapter"),
    )
}

#[test]
fn every_owner_below_the_source_root_links_one_local_chapter() {
    let registry = format!(
        "---\ntitle: Modules\n---\n\n{HEADER}{}| `src/author/module.ts` | Author chapter | A |\n| `src/shelf/module.ts` | [A](a.md) [B](b.md) | S |\n| `src/loan/module.ts` | [Loan](https://example.com/loan.md) | L |\n| `src/reader/module.ts` | [Reader](../../reader.md) | R |\n| `src/card/module.ts` | [Card](card.txt) | C |\n| `src/index/module.ts` | [[index]] | I |\n| `src/note/module.ts` | [Note][missing] | N |\n| `src/link/module.ts` | see https://example.com | L |\n| `lib/tool/module.ts` | none | T |\n| `src/guide/module.ts` | [Guide](../guide/book.md?view=1) | G |\n\n| Path | Chapter | Ownership |\n| --- | --- | --- |\n| `src/other/module.ts` | none | O |\n",
        entry("src/book/module.ts", "api/book.md"),
    );
    let found = run(RULE, "{}", "{}", &[(REGISTRY, registry.as_str())]).expect("the rule runs");
    let owner_chapter: Vec<host::Row> = rows(&found)
        .into_iter()
        .filter(|row| row.0 == "owner-chapter")
        .collect();

    assert_eq!(
        owner_chapter,
        [
            unlinked(8, "src/author/module.ts"),
            unlinked(9, "src/shelf/module.ts"),
            unlinked(10, "src/loan/module.ts"),
            unlinked(11, "src/reader/module.ts"),
            unlinked(12, "src/card/module.ts"),
            unlinked(13, "src/index/module.ts"),
            unlinked(14, "src/note/module.ts"),
            unlinked(15, "src/link/module.ts"),
        ]
    );
}

#[test]
fn the_documentation_root_and_the_layout_source_root_move_the_check() {
    let registry = format!(
        "{HEADER}{}{}",
        entry("lib/book/module.ts", "api/book.md"),
        entry("src/author/module.ts", "none.txt"),
    );
    let options = r#"{ "documentation-root": "handbook" }"#;
    let layout = r#"{ "layout": { "source-root": "lib" } }"#;
    let found =
        run(RULE, options, layout, &[(REGISTRY, registry.as_str())]).expect("the rule runs");
    let owner_chapter: Vec<host::Row> = rows(&found)
        .into_iter()
        .filter(|row| row.0 == "owner-chapter")
        .collect();

    assert_eq!(owner_chapter, [unlinked(3, "lib/book/module.ts")]);
}

#[test]
fn owners_are_checked_when_the_source_root_is_the_project_root() {
    let registry = format!(
        "{HEADER}{}{}",
        entry("book/module.ts", "api/book.md"),
        entry("author/module.ts", "author.txt"),
    );
    let layout = r#"{ "layout": { "source-root": "" } }"#;
    let found = run(RULE, "{}", layout, &[(REGISTRY, registry.as_str())]).expect("the rule runs");
    let owner_chapter: Vec<host::Row> = rows(&found)
        .into_iter()
        .filter(|row| row.0 == "owner-chapter")
        .collect();

    assert_eq!(owner_chapter, [unlinked(4, "author/module.ts")]);
}

#[test]
fn a_build_folder_below_the_source_root_is_read() {
    let registry = format!(
        "{HEADER}| `src/infrastructure/build/composition.ts` | [Build](build.md) | Platform |\n"
    );
    let module = module("CompositionModule");
    let files = [
        ("src/infrastructure/build/composition.ts", module.as_str()),
        (REGISTRY, registry.as_str()),
    ];

    assert!(
        check(RULE, "{}", &files)
            .iter()
            .all(|row| !row.3.contains("source module")),
        "the module below build/ is found"
    );
}

#[test]
fn a_custom_skip_list_replaces_the_default() {
    let generated = module("GeneratedModule");
    let vendored = module("VendoredModule");
    let files = [
        ("src/generated/module.ts", generated.as_str()),
        ("src/node_modules/x/module.ts", vendored.as_str()),
        (REGISTRY, HEADER),
    ];
    let missing = |languages: &str| -> Vec<String> {
        rows(
            &run_with(
                RULE,
                &Request {
                    languages,
                    ..Request::default()
                },
                &files,
            )
            .expect("the rule runs"),
        )
        .into_iter()
        .filter_map(|row| {
            row.3
                .strip_prefix(&format!("{REGISTRY} is missing source module: "))
                .map(str::to_owned)
        })
        .collect()
    };

    assert_eq!(missing(""), ["src/generated/module.ts"]);
    assert_eq!(missing(SKIP_GENERATED), ["src/node_modules/x/module.ts"]);
}

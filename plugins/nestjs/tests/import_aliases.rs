//! `nestjs/import-aliases` against synthetic source trees.

mod host;

use host::{Row, SKIP_GENERATED, check, error, paths_with, row, rows, run};

const RULE: &str = "import-aliases";

fn owner_alias(path: &str, specifier: &str) -> Row {
    row(
        "owner-alias",
        path,
        None,
        &format!("{path} loads {specifier}; use a canonical source-owner alias"),
    )
}

fn enum_file(path: &str, specifier: &str) -> Row {
    row(
        "entity-enum-barrel",
        path,
        None,
        &format!(
            "{path} loads the enum file {specifier}; import the enum through its owner's enum barrel"
        ),
    )
}

fn own_barrel(path: &str, specifier: &str, barrel: &str) -> Row {
    row(
        "entity-enum-barrel",
        path,
        None,
        &format!(
            "{path} loads its own resource's enum barrel through {specifier}; import it as {barrel}"
        ),
    )
}

#[test]
fn source_root_paths_deep_and_crossing_relative_loads_are_reported() {
    let service = "src/modules/book/service.ts";
    let plain = "src/modules/plain.ts";
    let loader = "src/shared/loader.ts";
    let types = "src/runtime/types.d.ts";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (
                    service,
                    "import { A } from \"src/shared/a\";\n\
                     import type { B } from \"src\";\n\
                     import { C } from \"../../modules/author/c\";\n\
                     import { D } from \"../../shared/d\";\n\
                     export * from \"../../runtime/e\";\n\
                     const f = import(\"src/config/f\");\n\
                     type TG = import(\"src/config/g\").G;\n",
                ),
                (
                    plain,
                    "import { H } from \"../shared/h\";\nimport I = require(\"../config/i\");\n",
                ),
                (
                    loader,
                    "export const K = require(\"../config/k\");\nexport const L = require(\"./l\");\n",
                ),
                (types, "declare const M: typeof import(\"../shared/m\");\n"),
            ]
        ),
        [
            owner_alias(service, "src/shared/a"),
            owner_alias(service, "src"),
            owner_alias(service, "../../modules/author/c"),
            owner_alias(service, "../../shared/d"),
            owner_alias(service, "../../runtime/e"),
            owner_alias(service, "src/config/f"),
            owner_alias(service, "src/config/g"),
            owner_alias(plain, "../shared/h"),
            owner_alias(plain, "../config/i"),
            owner_alias(types, "../shared/m"),
            owner_alias(loader, "../config/k"),
        ]
    );
}

#[test]
fn loads_within_an_owner_aliases_packages_and_files_outside_the_source_root_pass() {
    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (
                    "src/modules/book/service.ts",
                    "import { Helper } from \"./helper\";\n\
                     import { Author } from \"../author/service\";\n\
                     import { VALUE } from \"@shared/value\";\n\
                     import type { Repository } from \"typeorm\";\n\
                     export * from \".\";\n",
                ),
                (
                    "src/shared/value.ts",
                    "export const VALUE = require(\"./constant\");\n",
                ),
                ("src/main.ts", "import { x } from \"../outside\";\n"),
                (
                    "src/node_modules/x/index.ts",
                    "import { y } from \"src/shared/value\";\n",
                ),
                (
                    "test/unit/book.test.ts",
                    "import { Service } from \"src/modules/book/service\";\n",
                ),
                (
                    "src/broken.ts",
                    "import { z } from \"src/shared/value\";\nclass {"
                ),
            ]
        ),
        []
    );
}

#[test]
fn entities_that_load_enum_files_are_reported() {
    let module = "src/modules/domain/book/entity/entity.ts";
    let infrastructure = "src/infrastructure/library/entity/entity.ts";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (
                    module,
                    "import { EBookState } from \"../enum/state.enum\";\n\
                     import { EAuthorKind } from \"@modules/domain/author/enum/kind.enum\";\n\
                     import { EBookMode } from \"../enum/mode.enum.js\";\n\
                     import type { EAuthorScope } from \"@modules/domain/author/enum/scope.enum\";\n\
                     export { EBookLevel } from \"../enum/level.enum\";\n\
                     export class BookEntity {}\n",
                ),
                (
                    infrastructure,
                    "import { EAuthorKind } from \"src/modules/domain/author/enum/kind.enum\";\n",
                ),
            ]
        ),
        [
            enum_file(infrastructure, "src/modules/domain/author/enum/kind.enum"),
            owner_alias(infrastructure, "src/modules/domain/author/enum/kind.enum"),
            enum_file(module, "../enum/state.enum"),
            enum_file(module, "@modules/domain/author/enum/kind.enum"),
            enum_file(module, "../enum/mode.enum.js"),
            enum_file(module, "@modules/domain/author/enum/scope.enum"),
            enum_file(module, "../enum/level.enum"),
        ]
    );
}

#[test]
fn entities_that_load_their_own_enum_barrel_through_an_alias_are_reported() {
    let module = "src/modules/domain/book/entity/entity.ts";
    let infrastructure = "src/infrastructure/library/entity/entity.ts";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (
                    module,
                    "import { EBookState } from \"@modules/domain/book/enum\";\n\
                     import type { EBookMode } from \"@modules/domain/book/enum/index\";\n\
                     import { EBookLevel } from \"@modules/domain/book/enum/\";\n",
                ),
                (
                    infrastructure,
                    "import { ELibraryKind } from \"@infrastructure/library/enum\";\n",
                ),
            ]
        ),
        [
            own_barrel(infrastructure, "@infrastructure/library/enum", "../enum"),
            own_barrel(module, "@modules/domain/book/enum", "../enum"),
            own_barrel(module, "@modules/domain/book/enum/index", "../enum"),
            own_barrel(module, "@modules/domain/book/enum/", "../enum"),
        ]
    );
}

#[test]
fn entities_that_load_enum_barrels_and_other_files_that_load_enum_files_pass() {
    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (
                    "src/modules/domain/book/entity/entity.ts",
                    "import { EBookMode } from \"../enum\";\n\
                     import { EAuthorKind } from \"@modules/domain/author/enum\";\n\
                     import { ESharedLevel } from \"@shared/enum\";\n\
                     import { Column } from \"typeorm\";\n\
                     import { X } from \"package/state.enum\";\n",
                ),
                (
                    "src/modules/domain/book/chapter/entity/entity.ts",
                    "import { EChapterState } from \"../enum\";\n\
                     import { EBookState } from \"@modules/domain/book/enum\";\n",
                ),
                (
                    "src/modules/domain/book/service.ts",
                    "import { EBookState } from \"./enum/state.enum\";\n",
                ),
            ]
        ),
        []
    );
}

#[test]
fn the_folder_and_kind_options_change_what_an_entity_is() {
    let model = "src/modules/domain/book/model/model.ts";
    let text = "import { A } from \"../kinds/state.kind\";\n\
                import { B } from \"@modules/domain/book/kinds\";\n\
                import { C } from \"../enum/state.enum\";\n";
    let options = r#"{ "entity-folder": "model", "enum-folder": "kinds", "enum-kind": "kind" }"#;

    assert_eq!(
        check(RULE, options, &[(model, text)]),
        [
            enum_file(model, "../kinds/state.kind"),
            own_barrel(model, "@modules/domain/book/kinds", "../kinds"),
        ]
    );
}

#[test]
fn the_layout_names_the_source_root_owners_and_aliases() {
    let service = "app/catalog/book/service.ts";
    let layout = r#"{ "layout": {
        "source-root": "app",
        "source-owners": ["catalog", "shared"],
        "path-aliases": { "~catalog": "app/catalog" }
    } }"#;
    let files = [
        (
            service,
            "import { A } from \"app/shared/a\";\n\
             import { B } from \"../../shared/b\";\n\
             import { C } from \"src/shared/c\";\n",
        ),
        (
            "app/catalog/book/entity/entity.ts",
            "import { D } from \"~catalog/author/enum/kind.enum\";\n",
        ),
        ("app/vendor/x.ts", "import { E } from \"../catalog/e\";\n"),
    ];

    assert_eq!(
        rows(&run(RULE, "{}", layout, &files).expect("the rule runs")),
        [
            row(
                "entity-enum-barrel",
                "app/catalog/book/entity/entity.ts",
                None,
                "app/catalog/book/entity/entity.ts loads the enum file ~catalog/author/enum/kind.enum; import the enum through its owner's enum barrel",
            ),
            owner_alias(service, "app/shared/a"),
            owner_alias(service, "../../shared/b"),
        ]
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "entity": "x" }"#, &[]).contains("entity"));
}

#[test]
fn a_relative_import_inside_its_owner_passes_when_the_source_root_is_the_project_root() {
    let layout = r#"{ "layout": { "source-root": "", "source-owners": ["catalog", "shared"] } }"#;
    let service = "catalog/book/service.ts";
    let files = [
        (
            service,
            "import { A } from \"../author/a\";\nimport { B } from \"../../shared/b\";\n",
        ),
        ("catalog/author/a.ts", "export const A = 1;\n"),
    ];

    assert_eq!(
        rows(&run(RULE, "{}", layout, &files).expect("the rule runs")),
        [owner_alias(service, "../../shared/b")]
    );
}

#[test]
fn the_language_skip_list_decides_which_folders_are_read() {
    let text = "import { y } from \"src/shared/value\";\n";
    let files = [
        ("src/generated/a.ts", text),
        ("src/modules/build/b.ts", text),
        ("src/node_modules/x/index.ts", text),
    ];

    assert_eq!(
        paths_with(RULE, "{}", "", &files),
        ["src/generated/a.ts", "src/modules/build/b.ts"]
    );
    assert_eq!(
        paths_with(RULE, "{}", SKIP_GENERATED, &files),
        ["src/modules/build/b.ts", "src/node_modules/x/index.ts"]
    );
}

#[test]
fn the_entity_and_enum_folders_are_single_names() {
    for option in ["entity-folder", "enum-folder"] {
        for value in ["domain/entity", ""] {
            let message = error(RULE, &format!(r#"{{ "{option}": {value:?} }}"#), &[]);

            assert!(message.contains(option), "{message}");
            assert!(message.contains("single name"), "{message}");
        }
    }
}

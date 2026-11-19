//! `nestjs/custom-routes` against synthetic controllers.

mod host;

use host::{SKIP_GENERATED, check, error, paths_with, row};

const RULE: &str = "custom-routes";
const CONTROLLER: &str = "src/modules/book/book.controller.ts";

fn message(name: &str) -> String {
    format!(
        "{CONTROLLER} uses the raw Nest {name} route decorator; declare custom handlers with @ApiRouteCustom"
    )
}

#[test]
fn a_named_route_import_is_reported_at_its_specifier() {
    let text = "import { Controller,\n  Get } from \"@nestjs/common\";\n@Controller()\nexport class BookController {}\n";

    assert_eq!(
        check(RULE, "{}", &[(CONTROLLER, text)]),
        [row("", CONTROLLER, Some(2), &message("@Get"))]
    );
}

#[test]
fn re_exports_and_subpaths_are_reported() {
    let text = "export { Post as Create } from \"@nestjs/common/decorators\";\nexport * from \"@nestjs/common\";\n";

    assert_eq!(
        check(RULE, "{}", &[(CONTROLLER, text)]),
        [
            row("", CONTROLLER, Some(1), &message("@Post")),
            row("", CONTROLLER, Some(2), &message("re-exported")),
        ]
    );
}

#[test]
fn reads_from_namespace_default_and_require_bindings_are_reported() {
    let text = "import * as nest from \"@nestjs/common\";\n\
                import common from \"@nestjs/common\";\n\
                import req = require(\"@nestjs/common\");\n\
                const a = nest.Get;\n\
                const b = common[\"Put\"];\n\
                const { Delete, Injectable } = req;\n\
                const c = nest.Injectable;\n";

    assert_eq!(
        check(RULE, "{}", &[(CONTROLLER, text)]),
        [
            row("", CONTROLLER, Some(4), &message("@Get")),
            row("", CONTROLLER, Some(5), &message("@Put")),
            row("", CONTROLLER, Some(6), &message("@Delete")),
        ]
    );
}

#[test]
fn other_packages_declarations_skipped_folders_and_broken_files_pass() {
    let named = "import { Get } from \"@nestjs/common\";\n";
    let other = "import { Get } from \"@nestjs/commons\";\nimport { Injectable } from \"@nestjs/common\";\n";

    assert_eq!(
        check(
            RULE,
            "{}",
            &[
                (CONTROLLER, other),
                ("src/types/nest.d.ts", named),
                ("src/node_modules/x/index.ts", named),
                ("test/book.test.ts", named),
                (
                    "src/broken.ts",
                    "import { Get } from \"@nestjs/common\";\nclass {"
                ),
            ]
        ),
        []
    );
}

#[test]
fn options_change_the_package_vocabulary_and_replacement() {
    let text = "import { Get, Fetch } from \"@scope/web\";\n";
    let options =
        r#"{ "package": "@scope/web", "route-decorators": ["Fetch"], "replacement": "@Route" }"#;

    assert_eq!(
        check(RULE, options, &[(CONTROLLER, text)]),
        [row(
            "",
            CONTROLLER,
            Some(1),
            &format!(
                "{CONTROLLER} uses the raw Nest @Fetch route decorator; declare custom handlers with @Route"
            )
        )]
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "routes": [] }"#, &[]).contains("routes"));
}

#[test]
fn an_absent_package_follows_the_layout_and_an_empty_one_is_an_error() {
    let text = "import { Get } from \"@scope/web\";\n";
    let layout = r#"{ "layout": { "nest-common-package": "@scope/web" } }"#;
    let found = host::run(RULE, "{}", layout, &[(CONTROLLER, text)]).expect("the rule runs");

    assert_eq!(host::rows(&found).len(), 1);
    assert!(
        error(RULE, r#"{ "package": "" }"#, &[(CONTROLLER, text)]).contains("`package` is empty")
    );
}

#[test]
fn the_language_skip_list_decides_which_folders_are_read() {
    let named = "import { Get } from \"@nestjs/common\";\n";
    let files = [
        ("src/generated/a.controller.ts", named),
        ("src/modules/build/b.controller.ts", named),
        ("src/node_modules/x/index.ts", named),
    ];

    assert_eq!(
        paths_with(RULE, "{}", "", &files),
        [
            "src/generated/a.controller.ts",
            "src/modules/build/b.controller.ts"
        ]
    );
    assert_eq!(
        paths_with(RULE, "{}", SKIP_GENERATED, &files),
        [
            "src/modules/build/b.controller.ts",
            "src/node_modules/x/index.ts"
        ]
    );
}

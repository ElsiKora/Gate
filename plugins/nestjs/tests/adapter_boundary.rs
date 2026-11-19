//! `nestjs/adapter-boundary` against synthetic adapter loads.

mod support;

use support::{Row, check, error, row};

const RULE: &str = "adapter-boundary";
const FEATURE: &str = "src/modules/user/user.service.ts";
const OPTIONS: &str = r#"{
    "external-packages": ["pg"],
    "external-package-prefixes": ["@cloud/"],
    "inward-owner-prefixes": ["src/modules/"]
}"#;

fn external(path: &str, specifier: &str) -> Row {
    row(
        "external-package",
        path,
        None,
        &format!("{path} imports external adapter package {specifier} outside infrastructure"),
    )
}

fn inward(path: &str, specifier: &str) -> Row {
    row(
        "inward-infrastructure",
        path,
        None,
        &format!("{path} imports infrastructure adapter {specifier} into an inward owner"),
    )
}

#[test]
fn without_packages_or_owners_nothing_is_checked() {
    let text = "import { Pool } from \"pg\";\nimport { Db } from \"@infrastructure/db\";\n";

    assert_eq!(check(RULE, "{}", &[(FEATURE, text)]), []);
}

#[test]
fn external_packages_load_only_in_infrastructure() {
    let text = "import { Pool } from \"pg\";\n\
                import type { Client } from \"pg/lib/client\";\n\
                import { X } from \"pgx\";\n\
                const storage = require(\"@cloud/storage\");\n\
                export const ALL = [Pool, X, storage];\n";
    let adapter = "import { Pool } from \"pg\";\nexport const POOL = Pool;\n";

    assert_eq!(
        check(
            RULE,
            OPTIONS,
            &[(FEATURE, text), ("src/infrastructure/db/pool.ts", adapter)]
        ),
        [
            external(FEATURE, "pg"),
            external(FEATURE, "pg/lib/client"),
            external(FEATURE, "@cloud/storage"),
        ]
    );
}

#[test]
fn inward_owners_load_no_infrastructure_by_alias_root_spelling_or_relative_path() {
    let text = "import { Db } from \"@infrastructure/db\";\n\
                import { All } from \"@infrastructure\";\n\
                import { Pool } from \"../../infrastructure/db/pool\";\n\
                import { Other } from \"src/infrastructure/db\";\n\
                import { Folder } from \"../../infrastructure\";\n\
                import { Near } from \"@infrastructurex\";\n\
                import { Sibling } from \"../../infrastructurex/a\";\n\
                export const ALL = [Db, All, Pool, Other, Folder, Near, Sibling];\n";
    let outer = (
        "src/runtime/main.ts",
        "import { Db } from \"@infrastructure/db\";\n",
    );

    assert_eq!(
        check(RULE, OPTIONS, &[(FEATURE, text), outer]),
        [
            inward(FEATURE, "@infrastructure/db"),
            inward(FEATURE, "@infrastructure"),
            inward(FEATURE, "../../infrastructure/db/pool"),
            inward(FEATURE, "src/infrastructure/db"),
            inward(FEATURE, "../../infrastructure"),
        ]
    );
}

#[test]
fn a_custom_alias_of_the_layout_resolves_to_the_infrastructure_folder() {
    let text = "import { Db } from \"~infra/db\";\n\
                import { Shared } from \"~shared/db\";\n";
    let layout = r#"{ "layout": { "path-aliases": { "~infra": "src/infrastructure", "~shared": "src/shared" } } }"#;
    let findings = support::run(RULE, OPTIONS, layout, &[(FEATURE, text)]).expect("the rule runs");

    assert_eq!(support::rows(&findings), [inward(FEATURE, "~infra/db")]);
}

#[test]
fn the_infrastructure_prefix_sets_the_folder() {
    let options = r#"{
        "external-packages": ["pg"],
        "infrastructure-prefix": "src/adapters/",
        "inward-owner-prefixes": ["src/modules/"]
    }"#;
    let feature =
        "import { Db } from \"../../adapters/db\";\nimport { I } from \"@infrastructure/db\";\n";
    let adapter = "import { Pool } from \"pg\";\n";

    assert_eq!(
        check(
            RULE,
            options,
            &[
                (FEATURE, feature),
                ("src/adapters/db/pool.ts", adapter),
                ("src/infrastructure/db/pool.ts", adapter),
            ]
        ),
        [
            external("src/infrastructure/db/pool.ts", "pg"),
            inward(FEATURE, "../../adapters/db"),
        ]
    );

    let message = error(RULE, r#"{ "externalPackages": [] }"#, &[]);
    assert!(message.contains("externalPackages"), "{message}");
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "import { Pool } from \"pg\";\nexport const POOL = Pool;\n";
    let generated = "src/modules/generated/user/user.service.ts";
    let vendored = "src/node_modules/pkg/index.ts";
    let files = [(generated, text), (vendored, text)];
    let paths = |languages: &str| support::paths_with(RULE, OPTIONS, "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

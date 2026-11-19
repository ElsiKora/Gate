//! `nestjs/import-direction` against synthetic dependency contracts.

mod support;

use support::{Row, check, error, row};

const RULE: &str = "import-direction";
const DOMAIN: &str = "src/modules/domain/user/user.service.ts";

fn loads(aspect: &str, path: &str, specifier: &str, directory: &str) -> Row {
    row(
        aspect,
        path,
        None,
        &format!("{path} loads {specifier} outside its {directory} dependency contract"),
    )
}

fn domain(specifier: &str) -> Row {
    loads("domain-direction", DOMAIN, specifier, "src/modules/domain")
}

const CONTRACT: &str = r#"{ "contracts": [{
    "source-directory": "src/modules/domain",
    "aspect": "domain-direction",
    "forbidden-owner-paths": ["src/modules/public"],
    "forbidden-packages": ["@scope/swagger"],
    "forbidden-package-prefixes": ["@cloud/"],
    "forbidden-source-paths": ["src/shared/secret.ts"]
}] }"#;

#[test]
fn forbidden_owners_files_and_packages_are_reported_in_load_order() {
    let text = "import { A } from \"../../public/a\";\n\
                import { B } from \"@modules/public/b\";\n\
                import { C } from \"src/modules/public\";\n\
                import { D } from \"@modules/publication\";\n\
                import { E } from \"@scope/swagger/dist\";\n\
                import { F } from \"@scope/swaggerx\";\n\
                import { G } from \"@cloud/storage\";\n\
                import { H } from \"@shared/secret.js\";\n\
                import { I } from \"@shared/secret/other\";\n\
                export const ALL = [A, B, C, D, E, F, G, H, I];\n";
    let outside = (
        "src/modules/public/a.ts",
        "import { X } from \"@scope/swagger\";\n",
    );

    assert_eq!(
        check(RULE, CONTRACT, &[(DOMAIN, text), outside]),
        [
            domain("../../public/a"),
            domain("@modules/public/b"),
            domain("src/modules/public"),
            domain("@scope/swagger/dist"),
            domain("@cloud/storage"),
            domain("@shared/secret.js"),
        ]
    );
}

#[test]
fn type_loads_and_declaration_files_follow_the_contract_flags() {
    let typed = "import type { T } from \"@modules/public/t\";\nexport type U = T;\n";
    let declaration = "import { T } from \"@modules/public/t\";\n";
    let files = [
        (DOMAIN, typed),
        ("src/modules/domain/user/user.d.ts", declaration),
    ];
    let flags = r#"{ "contracts": [{
        "source-directory": "src/modules/domain",
        "forbidden-owner-paths": ["src/modules/public"],
        "forbidden-packages": [],
        "runtime-values-only": true,
        "include-declaration-files": false
    }] }"#;
    let contract = r#"{ "contracts": [{
        "source-directory": "src/modules/domain",
        "forbidden-owner-paths": ["src/modules/public"],
        "forbidden-packages": []
    }] }"#;

    assert_eq!(check(RULE, flags, &files), []);
    assert_eq!(
        check(RULE, contract, &files),
        [
            loads(
                "owner-contract",
                "src/modules/domain/user/user.d.ts",
                "@modules/public/t",
                "src/modules/domain"
            ),
            loads(
                "owner-contract",
                DOMAIN,
                "@modules/public/t",
                "src/modules/domain"
            ),
        ]
    );
}

#[test]
fn allowed_files_folders_and_edges_win() {
    let text = "import { A } from \"../../public\";\nexport const B = A;\n";
    let allowed = r#"{ "contracts": [{
        "source-directory": "src/modules/domain",
        "forbidden-owner-paths": ["src/modules/public"],
        "forbidden-packages": [],
        "allowed-source-paths": ["src/modules/domain/a.ts"],
        "allowed-source-directories": ["src/modules/domain/b"],
        "allowed-dependencies": [
            { "source-path": "src/modules/domain/user/user.service.ts", "target-paths": ["src/modules/public/index.ts"] }
        ]
    }] }"#;
    let files = [
        (DOMAIN, text),
        (
            "src/modules/domain/a.ts",
            "import { A } from \"../public\";\n",
        ),
        (
            "src/modules/domain/b/c.ts",
            "import { A } from \"../../public\";\n",
        ),
        (
            "src/modules/domain/d.ts",
            "import { A } from \"../public\";\n",
        ),
    ];

    assert_eq!(
        check(RULE, allowed, &files),
        [loads(
            "owner-contract",
            "src/modules/domain/d.ts",
            "../public",
            "src/modules/domain"
        )]
    );
}

#[test]
fn every_contract_that_forbids_a_load_reports_it() {
    let both = r#"{ "contracts": [
        { "source-directory": "src/modules", "aspect": "shared-purity", "forbidden-owner-paths": [], "forbidden-packages": ["pg"] },
        { "source-directory": "src/modules/domain", "forbidden-owner-paths": [], "forbidden-packages": ["pg"] }
    ] }"#;
    let text = "import { Pool } from \"pg\";\nexport const POOL = Pool;\n";

    assert_eq!(
        check(RULE, both, &[(DOMAIN, text)]),
        [
            loads("shared-purity", DOMAIN, "pg", "src/modules"),
            loads("owner-contract", DOMAIN, "pg", "src/modules/domain"),
        ]
    );
}

#[test]
fn malformed_contracts_are_rejected() {
    let missing =
        r#"{ "contracts": [{ "source-directory": "src", "forbidden-owner-paths": [] }] }"#;
    let message = error(RULE, missing, &[]);
    assert!(message.contains("forbidden-packages"), "{message}");

    let aspect = r#"{ "contracts": [{ "source-directory": "src", "aspect": "other", "forbidden-owner-paths": [], "forbidden-packages": [] }] }"#;
    let message = error(RULE, aspect, &[]);
    assert!(message.contains("other"), "{message}");
}

#[test]
fn a_missing_or_empty_contracts_option_is_an_error() {
    let missing = error(RULE, "{}", &[]);
    assert!(missing.contains("contracts"), "{missing}");

    let empty = error(RULE, r#"{ "contracts": [] }"#, &[]);
    assert!(
        empty.contains("option `contracts` lists nothing"),
        "{empty}"
    );
}

#[test]
fn a_source_directory_without_files_binds_nothing() {
    // A shared contract may name a folder a project does not have yet: that is no finding.
    let text = "import { X } from \"@scope/swagger\";\nexport const Y = X;\n";

    assert_eq!(
        check(RULE, CONTRACT, &[("src/modules/public/a.ts", text)]),
        []
    );
    assert_eq!(check(RULE, CONTRACT, &[]), []);
}

#[test]
fn an_allowed_source_directory_of_the_project_root_allows_every_file() {
    let options = CONTRACT.replace(
        r#""forbidden-source-paths""#,
        r#""allowed-source-directories": [""], "forbidden-source-paths""#,
    );
    let text = "import { A } from \"../../public/a\";\n";

    assert_eq!(check(RULE, &options, &[(DOMAIN, text)]), []);
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "import { X } from \"@scope/swagger\";\nexport const Y = X;\n";
    let generated = "src/modules/domain/generated/user/user.service.ts";
    let vendored = "src/modules/domain/node_modules/pkg/index.ts";
    let files = [(generated, text), (vendored, text)];
    let paths = |languages: &str, files: &[(&str, &str)]| {
        support::paths_with(RULE, CONTRACT, "{}", languages, files)
    };

    assert_eq!(paths("{}", &files), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED, &files), [vendored]);
    // A source directory whose only file sits in a skipped folder binds nothing.
    assert_eq!(
        paths(support::SKIP_GENERATED, &[(generated, text)]),
        Vec::<String>::new()
    );
}

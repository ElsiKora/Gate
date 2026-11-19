//! `nestjs/advisory-lock-manifest` against synthetic manifests, enums and markers.

mod support;

use support::{Row, check, row};

const RULE: &str = "advisory-lock-manifest";
const MANIFEST: &str = "src/common/advisory-lock/registry.manifest.json";
const ENUM: &str = "src/common/advisory-lock/enum/operation.enum.ts";

const OPERATIONS: [&str; 10] = [
    "SESSION_LOCK_EXCLUSIVE",
    "SESSION_LOCK_SHARED",
    "SESSION_TRY_LOCK_EXCLUSIVE",
    "SESSION_TRY_LOCK_SHARED",
    "SESSION_UNLOCK_EXCLUSIVE",
    "SESSION_UNLOCK_SHARED",
    "TRANSACTION_LOCK_EXCLUSIVE",
    "TRANSACTION_LOCK_SHARED",
    "TRANSACTION_TRY_LOCK_EXCLUSIVE",
    "TRANSACTION_TRY_LOCK_SHARED",
];

fn operation_enum() -> String {
    let mut text = String::from("export enum EDatabaseAdvisoryLockOperation {\n");

    for operation in OPERATIONS {
        text.push_str("  ");
        text.push_str(operation);
        text.push_str(" = \"");
        text.push_str(operation);
        text.push_str("\",\n");
    }

    text.push_str("}\nObject.freeze(EDatabaseAdvisoryLockOperation);\n");
    text
}

const VALID_MANIFEST: &str = r#"{
  "schemaVersion": 1,
  "resourceDomain": "shop",
  "families": [
    {
      "name": "CART",
      "familyId": 10,
      "scope": "PRODUCTION",
      "resourceKind": "DYNAMIC",
      "rank": 1,
      "predecessors": [],
      "allowedOperations": ["TRANSACTION_LOCK_EXCLUSIVE"],
      "resources": []
    },
    {
      "name": "STOCK",
      "familyId": -20,
      "scope": "PRODUCTION",
      "resourceKind": "STATIC",
      "rank": 2,
      "predecessors": ["CART"],
      "allowedOperations": [],
      "resources": [
        { "name": "REBUILD", "resourceId": 7, "allowedOperations": ["SESSION_TRY_LOCK_EXCLUSIVE"] }
      ]
    }
  ]
}
"#;

fn manifest(aspect: &str, message: &str) -> Row {
    row(aspect, MANIFEST, None, message)
}

#[test]
fn a_valid_manifest_enum_and_marker_pass() {
    let enum_text = operation_enum();
    let files = [
        (MANIFEST, VALID_MANIFEST),
        (ENUM, enum_text.as_str()),
        (
            "src/stock/rebuild.ts",
            "// DATABASE_ADVISORY_LOCK_STATIC_KEY STOCK.REBUILD=-20,7\nexport {};\n",
        ),
    ];

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn an_untracked_manifest_is_a_configuration_finding_naming_it() {
    let manifest = "app/common/advisory-lock/registry.manifest.json";
    let findings = support::run(
        RULE,
        "{}",
        r#"{ "layout": { "source-root": "app" } }"#,
        &[(ENUM, "export {};\n")],
    )
    .expect("the rule runs");

    assert_eq!(
        support::rows(&findings),
        [row(
            "manifest",
            manifest,
            None,
            &format!(
                "The advisory-lock manifest {manifest} is not tracked; track it, set `manifest-path` to the manifest, or leave the rule off."
            ),
        )]
    );
}

#[test]
fn the_manifest_shape_metadata_and_predecessors_are_checked() {
    let enum_text = operation_enum();
    let reordered = VALID_MANIFEST.replace(
        "\"schemaVersion\": 1,\n  \"resourceDomain\": \"shop\",",
        "\"resourceDomain\": \"shop\",\n  \"schemaVersion\": 1,",
    );
    let duplicated = VALID_MANIFEST.replace("\"familyId\": -20", "\"familyId\": 10");
    let predecessor = VALID_MANIFEST.replace("\"rank\": 2", "\"rank\": 1");
    let static_operations = VALID_MANIFEST.replace(
        "\"allowedOperations\": [],",
        "\"allowedOperations\": [\"SESSION_LOCK_SHARED\"],",
    );
    let wide = VALID_MANIFEST.replace("\"resourceId\": 7", "\"resourceId\": 2147483648");
    let carriage = VALID_MANIFEST.replace('\n', "\r\n");
    let case = |text: &str| check(RULE, "{}", &[(MANIFEST, text), (ENUM, enum_text.as_str())]);

    assert_eq!(
        case(&reordered),
        [manifest("manifest", "Manifest has an invalid shape.")]
    );
    assert_eq!(
        case(&duplicated),
        [manifest("manifest", "Family STOCK is duplicated.")]
    );
    assert_eq!(
        case(&predecessor),
        [manifest(
            "manifest",
            "Family STOCK has an invalid predecessor CART."
        )]
    );
    assert_eq!(
        case(&static_operations),
        [manifest(
            "manifest",
            "Static family STOCK violates the operation/resource XOR."
        )]
    );
    assert_eq!(
        case(&wide),
        [manifest(
            "manifest",
            "Resource STOCK.REBUILD ID must be a signed int32."
        )]
    );
    assert_eq!(
        case(&carriage),
        [manifest(
            "manifest",
            "Manifest is not in its exact formatted form: it has a carriage return."
        )]
    );
    assert_eq!(
        case("{"),
        [manifest(
            "manifest",
            "Manifest is not valid JSON: EOF while parsing an object at line 1 column 1"
        )]
    );
}

#[test]
fn the_operation_enum_must_be_exact_and_frozen() {
    let path = |message: &str| row("operation-enum", ENUM, None, message);
    let full = operation_enum();
    let unfrozen = full.replace("Object.freeze(EDatabaseAdvisoryLockOperation);\n", "");
    let renamed = full.replace(
        "SESSION_LOCK_SHARED = \"SESSION_LOCK_SHARED\"",
        "SESSION_LOCK_SHARED = \"SHARED\"",
    );
    let quoted = full.replace("  SESSION_LOCK_SHARED =", "  \"SESSION_LOCK_SHARED\" =");
    let case = |text: &str| check(RULE, "{}", &[(MANIFEST, VALID_MANIFEST), (ENUM, text)]);

    assert_eq!(
        check(RULE, "{}", &[(MANIFEST, VALID_MANIFEST)]),
        [path("The operation enum is missing.")]
    );
    assert_eq!(
        case(&unfrozen),
        [path(
            "Operation enum does not contain the exact ten operations."
        )]
    );
    assert_eq!(
        case(&renamed),
        [path(
            "Operation enum does not contain the exact ten operations."
        )]
    );
    assert_eq!(
        case(&quoted),
        [path(
            "Operation enum members must have unquoted names and string initializers."
        )]
    );
    assert_eq!(
        case(&full.replace(
            "Object.freeze(EDatabaseAdvisoryLockOperation)",
            "Object.seal(EDatabaseAdvisoryLockOperation)"
        )),
        [path("Operation enum object is not frozen exactly once.")]
    );
}

#[test]
fn markers_match_the_manifest_their_paths_and_the_required_list() {
    let enum_text = operation_enum();
    let marker = "// DATABASE_ADVISORY_LOCK_STATIC_KEY STOCK.REBUILD=-20,7\n";
    let base = [(MANIFEST, VALID_MANIFEST), (ENUM, enum_text.as_str())];
    let with = |extra: &[(&'static str, &'static str)]| {
        let mut files = base.to_vec();
        files.extend_from_slice(extra);
        files
    };

    assert_eq!(
        check(
            RULE,
            "{}",
            &with(&[
                ("src/a.ts", "// DATABASE_ADVISORY_LOCK_STATIC_KEY stock\n"),
                (
                    "src/b.ts",
                    "x;\n// DATABASE_ADVISORY_LOCK_STATIC_KEY STOCK.REBUILD=-20,8\n"
                ),
            ])
        ),
        [
            row(
                "marker",
                "src/a.ts",
                Some(1),
                "Malformed static-key marker in src/a.ts."
            ),
            row(
                "marker",
                "src/b.ts",
                Some(2),
                "Static-key marker STOCK.REBUILD does not match the manifest."
            ),
        ]
    );
    assert_eq!(
        check(
            RULE,
            "{}",
            &with(&[("src/a.ts", marker), ("src/b.d.ts", marker)])
        ),
        [row(
            "marker",
            "src/b.d.ts",
            None,
            "Static-key marker STOCK.REBUILD is duplicated outside its exact migration paths."
        )]
    );

    let options = r#"{
        "marker-paths": { "STOCK.REBUILD": ["src/b.ts", "src/a.ts"] },
        "required-markers": ["STOCK.REBUILD", "STOCK.AUDIT"]
    }"#;
    assert_eq!(
        check(
            RULE,
            options,
            &with(&[("src/a.ts", marker), ("src/b.ts", marker)])
        ),
        [
            row(
                "marker",
                "src/a.ts",
                None,
                "Static-key marker STOCK.REBUILD does not match its exact migration paths."
            ),
            manifest(
                "marker",
                "Required static-key marker STOCK.AUDIT is absent."
            ),
        ]
    );
}

#[test]
fn the_paths_names_and_operations_are_options() {
    let options = r#"{
        "manifest-path": "locks/manifest.json",
        "operation-enum-path": "src/lock.enum.ts",
        "operation-enum-name": "ELock",
        "operations": ["SESSION_LOCK_EXCLUSIVE", "TRANSACTION_LOCK_EXCLUSIVE"],
        "marker-token": "LOCK_KEY"
    }"#;
    let manifest_text = VALID_MANIFEST.replace(
        "[\"SESSION_TRY_LOCK_EXCLUSIVE\"]",
        "[\"SESSION_LOCK_EXCLUSIVE\"]",
    );
    let enum_text = "export enum ELock {\n  SESSION_LOCK_EXCLUSIVE = \"SESSION_LOCK_EXCLUSIVE\",\n  TRANSACTION_LOCK_EXCLUSIVE = \"TRANSACTION_LOCK_EXCLUSIVE\",\n}\nObject.freeze(ELock);\n";
    let files = [
        ("locks/manifest.json", manifest_text.as_str()),
        ("src/lock.enum.ts", enum_text),
        ("src/a.ts", "// LOCK_KEY STOCK.REBUILD=-20,7\n"),
        ("src/b.ts", "// LOCK_KEY STOCK.REBUILD=1,7\n"),
    ];

    assert_eq!(
        check(RULE, options, &files),
        [row(
            "marker",
            "src/b.ts",
            Some(1),
            "Static-key marker STOCK.REBUILD does not match the manifest."
        )]
    );
    assert_eq!(
        check(
            RULE,
            options,
            &[
                ("locks/manifest.json", manifest_text.as_str()),
                (
                    "src/lock.enum.ts",
                    "export enum ELock {}\nObject.freeze(ELock);\n"
                )
            ]
        ),
        [row(
            "operation-enum",
            "src/lock.enum.ts",
            None,
            "Operation enum does not contain the exact two operations."
        )]
    );
}

#[test]
fn the_walk_reads_files_in_skipped_folders() {
    let enum_text = operation_enum();
    let malformed = "// DATABASE_ADVISORY_LOCK_STATIC_KEY stock\n";
    let built = "src/dist/stock/a.ts";
    let generated = "src/generated/stock/a.ts";
    let files = [
        (MANIFEST, VALID_MANIFEST),
        (ENUM, enum_text.as_str()),
        (built, malformed),
        (generated, malformed),
    ];
    let paths = |languages: &str| support::paths_with(RULE, "{}", "{}", languages, &files);

    assert_eq!(paths("{}"), [built, generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [built, generated]);
}

//! `nestjs/advisory-lock-boundary` against a synthetic executor, migrations, shell scripts and
//! sources.

mod support;

use support::{check, row};

const RULE: &str = "advisory-lock-boundary";
const MANIFEST: &str = "src/common/advisory-lock/registry.manifest.json";
const EXECUTOR: &str = "src/common/advisory-lock/function/execute.function.ts";

const MANIFEST_TEXT: &str = r#"{
 "schemaVersion": 1,
 "resourceDomain": "shop",
 "families": [
  {
   "name": "STOCK",
   "familyId": 4100,
   "scope": "PRODUCTION",
   "resourceKind": "STATIC",
   "rank": 1,
   "predecessors": [],
   "allowedOperations": [],
   "resources": [
    { "name": "REBUILD", "resourceId": 4200, "allowedOperations": ["TRANSACTION_LOCK_EXCLUSIVE"] },
    { "name": "AUDIT", "resourceId": 1, "allowedOperations": ["TRANSACTION_LOCK_EXCLUSIVE"] }
   ]
  }
 ]
}
"#;

const EXECUTOR_TEXT: &str = "export async function execute(manager: { query(sql: string, parameters: unknown[]): Promise<unknown> }, family: number, resource: number): Promise<void> {\n\
                             \x20 await manager.query(\"SELECT pg_catalog.pg_advisory_xact_lock($1::integer, $2::integer)\", [family, resource]);\n\
                             }\n";

const OPTIONS: &str = r#"{ "executor": "src/common/advisory-lock/function/execute.function.ts" }"#;

fn base() -> Vec<(&'static str, &'static str)> {
    vec![(MANIFEST, MANIFEST_TEXT), (EXECUTOR, EXECUTOR_TEXT)]
}

fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut files = base();
    files.extend_from_slice(extra);
    files
}

/// The executor, the manifest `manifest` and the `extra` files.
fn with_manifest<'a>(manifest: &'a str, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut files = vec![(MANIFEST, manifest), (EXECUTOR, EXECUTOR_TEXT)];
    files.extend_from_slice(extra);
    files
}

#[test]
fn the_executor_alone_passes_and_a_missing_executor_is_an_options_error() {
    assert_eq!(check(RULE, OPTIONS, &base()), []);

    let message = support::error(RULE, "{}", &base());
    assert!(message.contains("executor"), "{message}");
}

#[test]
fn advisory_sql_outside_the_executor_or_malformed_in_it_is_reported() {
    let found = check(
        RULE,
        OPTIONS,
        &[
            (MANIFEST, MANIFEST_TEXT),
            (
                EXECUTOR,
                "export const run = (db: { query(sql: string): void }) => db.query(\"SELECT pg_advisory_lock($1::bigint)\");\n",
            ),
            (
                "src/stock/rebuild.ts",
                "export const run = (db: { query(sql: string): void }) => db.query(\"SELECT pg_catalog.pg_advisory_xact_lock($1::integer, $2::integer)\");\n",
            ),
            (
                "test/stock.sql",
                "SELECT pg_advisory_lock(hashtext('stock'));\n",
            ),
            (
                "src/stock/dynamic.ts",
                "export const run = (db: { query(sql: string): void }, name: string) => db.query(\"SELECT pg_advisory_lock(\" + name + \")\");\n",
            ),
        ],
    );

    assert_eq!(
        found,
        [
            row(
                "executor",
                EXECUTOR,
                None,
                "src/common/advisory-lock/function/execute.function.ts contains a malformed executor operation pg_advisory_lock."
            ),
            row(
                "unresolved",
                "src/stock/dynamic.ts",
                None,
                "src/stock/dynamic.ts contains unresolved executable advisory SQL."
            ),
            row(
                "outside",
                "src/stock/rebuild.ts",
                None,
                "src/stock/rebuild.ts contains executable advisory SQL outside the exact allowlist."
            ),
            row(
                "derivation",
                "test/stock.sql",
                None,
                "test/stock.sql contains an alternate advisory-key derivation."
            ),
        ]
    );
}

#[test]
fn advisory_evidence_is_read_in_any_text_with_javascripts_case_folding() {
    // An argument that cannot be rebuilt is unresolved when a variable it reads holds advisory
    // evidence, read as `/iu` reads it: the long s (`\u{17f}`) folds to `s`, and `\u{15d}` to
    // nothing ASCII, whatever else the text holds.
    let sink = |key: &str| {
        format!(
            "export const run = (db: {{ query(sql: string): void }}, name: string) => {{ const key = \"SELECT {key}(\" + name; return db.query(key); }};\n"
        )
    };
    let (ascii, folded, other) = (
        sink("HASHTEXT"),
        sink("ha\u{17f}htext\u{e9}"),
        sink("ha\u{15d}htext\u{e9}"),
    );
    let found = check(
        RULE,
        OPTIONS,
        &with_manifest(
            MANIFEST_TEXT,
            &[
                ("src/stock/ascii.ts", &ascii),
                ("src/stock/folded.ts", &folded),
                ("src/stock/other.ts", &other),
            ],
        ),
    );
    let unresolved = |path: &str| {
        row(
            "unresolved",
            path,
            None,
            &format!("{path} contains unresolved executable advisory SQL."),
        )
    };

    assert_eq!(
        found,
        [
            unresolved("src/stock/ascii.ts"),
            unresolved("src/stock/folded.ts")
        ]
    );
}

#[test]
fn shell_commands_and_package_scripts_run_no_advisory_sql() {
    let found = check(
        RULE,
        OPTIONS,
        &with(&[
            (
                "package.json",
                r#"{ "scripts": { "lock": "psql -c 'SELECT pg_advisory_lock(1)'" } }"#,
            ),
            (
                "scripts/reset.sh",
                "SQL=\"SELECT pg_try_advisory_lock(2)\"\npsql -c \"$SQL\"\n",
            ),
            ("scripts/ok.sh", "echo pg_advisory_lock\n"),
        ]),
    );

    assert_eq!(
        found,
        [
            row(
                "shell",
                "package.json",
                None,
                "package.json contains executable advisory SQL in a shell command."
            ),
            row(
                "shell",
                "scripts/reset.sh",
                None,
                "scripts/reset.sh contains executable advisory SQL in a shell command."
            ),
        ]
    );
}

#[test]
fn registered_identifiers_and_structural_keys_stay_in_their_files() {
    let options = r#"{
        "executor": "src/common/advisory-lock/function/execute.function.ts",
        "contextual-resources": [{ "group": "STOCK", "name": "AUDIT", "id": 1 }],
        "inventory": ["src/common/advisory-lock/registry.manifest.json", "src/common/advisory-lock/key.ts"],
        "mint": {
            "structural": ["src/common/advisory-lock/key.ts"],
            "required": [{ "path": "src/common/advisory-lock/key.ts", "texts": ["mintKey"], "message": "does not retain the key mint." }]
        }
    }"#;
    let found = check(
        RULE,
        options,
        &with(&[
            (
                "src/common/advisory-lock/key.ts",
                "export const KEY = { family: \"STOCK\", familyId: 4100, resourceKey: 4200 };\n",
            ),
            (
                "src/stock/ids.ts",
                "export const FAMILY = 4000 + 100;\nexport const ONE = 1;\n",
            ),
            (
                "src/stock/key.ts",
                "export const KEY = { family: \"X\", familyId: 7, resourceKey: 8 };\n",
            ),
        ]),
    );

    assert_eq!(
        found,
        [
            row(
                "mint",
                "src/common/advisory-lock/key.ts",
                None,
                "src/common/advisory-lock/key.ts does not retain the key mint."
            ),
            row(
                "inventory",
                "src/stock/ids.ts",
                None,
                "src/stock/ids.ts contains registered numeric IDs outside the exact source allowlist."
            ),
            row(
                "mint",
                "src/stock/key.ts",
                None,
                "src/stock/key.ts constructs a structural advisory-lock key outside the mint allowlist."
            ),
        ]
    );
}

#[test]
fn a_registered_identifier_is_found_in_any_numeric_form_and_only_at_its_exact_value() {
    // The executor's `$1` is the contextual resource AUDIT: left out of the inventory here, and
    // renumbered 0 in the manifest of the second check.
    let options = r#"{
        "executor": "src/common/advisory-lock/function/execute.function.ts",
        "contextual-resources": [{ "group": "STOCK", "name": "AUDIT", "id": 1 }],
        "inventory": ["src/common/advisory-lock/registry.manifest.json"]
    }"#;
    let zero_options = r#"{
        "executor": "src/common/advisory-lock/function/execute.function.ts",
        "inventory": ["src/common/advisory-lock/registry.manifest.json"]
    }"#;
    let zero = MANIFEST_TEXT.replace("\"resourceId\": 1,", "\"resourceId\": 0,");
    let inventory = |path: &str| {
        row(
            "inventory",
            path,
            None,
            &format!("{path} contains registered numeric IDs outside the exact source allowlist."),
        )
    };

    let found = check(
        RULE,
        options,
        &with_manifest(
            MANIFEST_TEXT,
            &[
                ("src/a/hex.ts", "export const FAMILY = 0x1004;\n"),
                ("src/a/accent.ts", "export const A = \"\u{e9}4100\";\n"),
                ("src/a/word.ts", "export const A = \"x4100 4200n9\";\n"),
                ("src/a/exponent.ts", "export const RESOURCE = 42e2;\n"),
                ("src/a/sum.ts", "export const RESOURCE = 4199.5 + 0.5;\n"),
                (
                    "src/a/near.ts",
                    "export const A = 4.1005e3;\nexport const B = 0x1005;\nexport const C = 0 / 0;\n",
                ),
            ],
        ),
    );
    assert_eq!(
        found,
        [
            inventory("src/a/accent.ts"),
            inventory("src/a/exponent.ts"),
            inventory("src/a/hex.ts"),
            inventory("src/a/sum.ts"),
        ]
    );

    let found = check(
        RULE,
        zero_options,
        &with_manifest(
            &zero,
            &[
                ("src/a/negative.ts", "export const ZERO = -0;\n"),
                ("src/a/product.ts", "export const ZERO = -1 * 0;\n"),
                ("src/a/none.ts", "export const ONE = 2;\n"),
            ],
        ),
    );
    assert_eq!(
        found,
        [
            inventory("src/a/negative.ts"),
            inventory("src/a/product.ts")
        ]
    );
}

#[test]
fn migrations_need_the_ledger_and_required_boundaries_must_be_observed() {
    let options = r#"{
        "executor": "src/common/advisory-lock/function/execute.function.ts",
        "migrations": ["src/migration/1-baseline.ts"],
        "negative": "test/lock.test.ts"
    }"#;
    let found = check(RULE, options, &[(MANIFEST, MANIFEST_TEXT)]);

    assert_eq!(
        found,
        [
            row(
                "migration",
                "gate.toml",
                None,
                "Baseline migration boundaries need the ledger key they lock."
            ),
            row(
                "boundary",
                EXECUTOR,
                None,
                "Required source SQL boundary src/common/advisory-lock/function/execute.function.ts was not observed."
            ),
            row(
                "boundary",
                "src/migration/1-baseline.ts",
                None,
                "Required source SQL boundary src/migration/1-baseline.ts was not observed."
            ),
            row(
                "boundary",
                "test/lock.test.ts",
                None,
                "Required source SQL boundary test/lock.test.ts was not observed."
            ),
        ]
    );

    let ledger = r#"{
        "executor": "src/common/advisory-lock/function/execute.function.ts",
        "migrations": ["src/migration/1-baseline.ts"],
        "ledger": "STOCK.REBUILD"
    }"#;
    let migration = "export class Baseline { async up(runner: { query(sql: string): Promise<void> }): Promise<void> {\n\
                     \x20 await runner.query(\"SELECT pg_catalog.pg_advisory_xact_lock(4100, 4200)\");\n\
                     } }\n";
    let found = check(
        RULE,
        ledger,
        &with(&[("src/migration/1-baseline.ts", migration)]),
    );

    assert_eq!(found, []);
}

#[test]
fn unreadable_files_and_vocabularies() {
    let options = r#"{
        "executor": "src/common/advisory-lock/function/execute.function.ts",
        "sql-sinks": { "methods": ["run"], "tags": ["pgsql"] },
        "shell-executables": ["pgcli"]
    }"#;
    let found = check(
        RULE,
        options,
        &[
            (MANIFEST, MANIFEST_TEXT),
            (
                EXECUTOR,
                "export const run = (db: { run(sql: string): void }) => db.run(\"SELECT pg_catalog.pg_advisory_lock($1::integer, $2::integer)\");\n",
            ),
            (
                "src/stock/query.ts",
                "export const run = (db: { query(sql: string): void }) => db.query(\"SELECT pg_advisory_lock(1)\");\n",
            ),
            (
                "src/stock/tagged.ts",
                "declare const db: { pgsql(parts: TemplateStringsArray): string; sql(parts: TemplateStringsArray): string };\nexport const x = db.pgsql`SELECT pg_advisory_lock(1)`;\nexport const y = db.sql`SELECT pg_advisory_lock(2)`;\n",
            ),
            ("scripts/a.sh", "pgcli -c 'SELECT pg_advisory_lock(1)'\n"),
            ("scripts/b.sh", "psql -c 'SELECT pg_advisory_lock(1)'\n"),
            ("src/broken.ts", "export const = ;\n"),
        ],
    );

    assert_eq!(
        found,
        [
            row(
                "unreadable",
                "src/broken.ts",
                None,
                "The file cannot be parsed, so its executable advisory SQL cannot be checked."
            ),
            row(
                "outside",
                "src/stock/tagged.ts",
                None,
                "src/stock/tagged.ts contains executable advisory SQL outside the exact allowlist."
            ),
            row(
                "shell",
                "scripts/a.sh",
                None,
                "scripts/a.sh contains executable advisory SQL in a shell command."
            ),
        ]
    );
}

#[test]
fn the_walk_skips_the_typescript_skip_folders() {
    let text = "export const run = (db: { query(sql: string): void }) => db.query(\"SELECT pg_catalog.pg_advisory_xact_lock($1::integer, $2::integer)\");\n";
    let generated = "src/generated/stock/rebuild.ts";
    let vendored = "src/node_modules/pkg/rebuild.ts";
    let files = with(&[(generated, text), (vendored, text)]);
    let paths = |languages: &str| support::paths_with(RULE, OPTIONS, "{}", languages, &files);

    assert_eq!(paths("{}"), [generated]);
    assert_eq!(paths(support::SKIP_GENERATED), [vendored]);
}

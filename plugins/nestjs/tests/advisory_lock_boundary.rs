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

//! `nestjs/kind-imports` against synthetic source trees: each tree with the loads that pass and
//! the findings, at file and line, of those that do not.

mod host;

use std::collections::HashSet;

use host::{Row, check, error, row, rows};

const RULE: &str = "kind-imports";

/// The kinds of most trees: the default kinds less those no tree uses, with `config` and `entity`.
const KINDS: &str =
    r#"{ "kinds": ["config", "constant", "dto", "entity", "enum", "function", "interface"] }"#;

const FORMAT: &str = "src/modules/domain/book/function/format.function.ts";
const AVAILABLE: &str = "src/modules/domain/book/function/is/available.function.ts";
const KIND: &str = "src/modules/domain/book/function";

/// The base tree: the resource `book` with one kind folder, `function`, whose semantic subfolder
/// `is` has its own barrel, and a resource barrel. It gives no finding.
fn base() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            FORMAT,
            "export function bookFormat(): string {\n\treturn \"book\";\n}\n",
        ),
        (
            "src/modules/domain/book/function/index.ts",
            "export { bookFormat } from \"./format.function\";\nexport * from \"./is\";\n",
        ),
        (
            AVAILABLE,
            "import { bookFormat } from \"../format.function\";\n\nexport function bookIsAvailable(): boolean {\n\treturn bookFormat() === \"book\";\n}\n",
        ),
        (
            "src/modules/domain/book/function/is/index.ts",
            "export { bookIsAvailable } from \"./available.function\";\n",
        ),
        (
            "src/modules/domain/book/index.ts",
            "export * from \"./function\";\n",
        ),
    ]
}

/// The base tree with `files` added, each replacing a base file of the same path.
fn with(files: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut tree: Vec<_> = base()
        .into_iter()
        .filter(|(path, _)| !files.iter().any(|(added, _)| added == path))
        .collect();
    tree.extend_from_slice(files);
    tree
}

fn finding(aspect: &str, path: &str, line: u32, message: &str) -> Row {
    row(aspect, path, Some(line), &format!("{path} {message}"))
}

/// Fails when two findings share an aspect, a path, a line and a key: an exception keyed on the
/// specifier could not tell them apart.
fn assert_one_per_key(options: &str, tree: &[(&str, &str)]) {
    let findings = host::run(RULE, options, "{}", tree).expect("the rule runs");
    let mut seen = HashSet::new();
    for ((aspect, path, line, message), finding) in rows(&findings).into_iter().zip(&findings) {
        assert!(
            seen.insert((aspect, path, line, finding.key.clone())),
            "a second finding of the same aspect, line and key: {message}"
        );
    }
}

#[test]
fn the_base_tree_passes_with_the_listed_kinds_and_with_the_default_kinds() {
    assert_eq!(check(RULE, KINDS, &base()), []);
    assert_eq!(check(RULE, "{}", &base()), []);
}

#[test]
fn a_kind_folder_declaration_is_loaded_through_its_kind_barrel() {
    let file = "src/modules/domain/author/file.service.ts";
    let subfolder = "src/modules/domain/author/subfolder.service.ts";
    let wide = "src/modules/domain/author/wide.service.ts";
    let tree = with(&[
        (
            file,
            "import { bookFormat } from \"@modules/domain/book/function/format.function\";\n\nexport const AUTHOR_FILE_LABEL = bookFormat();\n",
        ),
        (
            "src/modules/domain/author/service.ts",
            "import { bookFormat } from \"@modules/domain/book/function\";\n\nexport const AUTHOR_LABEL = bookFormat();\n",
        ),
        (
            subfolder,
            "import { bookIsAvailable } from \"@modules/domain/book/function/is\";\n\nexport const AUTHOR_AVAILABLE = bookIsAvailable();\n",
        ),
        (
            wide,
            "import { bookFormat } from \"@modules/domain/book\";\n\nexport const AUTHOR_WIDE_LABEL = bookFormat();\n",
        ),
    ]);
    let expected = [
        finding(
            "kind-barrel",
            file,
            1,
            &format!(
                "loads @modules/domain/book/function/format.function; import bookFormat through the kind barrel {KIND}"
            ),
        ),
        finding(
            "kind-barrel",
            subfolder,
            1,
            &format!(
                "loads @modules/domain/book/function/is; import bookIsAvailable through the kind barrel {KIND}"
            ),
        ),
        finding(
            "kind-barrel",
            wide,
            1,
            &format!(
                "loads @modules/domain/book; import bookFormat through the kind barrel {KIND}"
            ),
        ),
    ];
    assert_eq!(check(RULE, KINDS, &tree), expected);
    // The default kinds hold `function` too.
    assert_eq!(check(RULE, "{}", &tree), expected);
}

#[test]
fn an_importer_inside_a_kind_folder_names_the_file() {
    let unavailable = "src/modules/domain/book/function/is/unavailable.function.ts";
    let tree = with(&[
        (
            AVAILABLE,
            "import { bookFormat } from \"..\";\n\nexport function bookIsAvailable(): boolean {\n\treturn bookFormat() === \"book\";\n}\n",
        ),
        (
            unavailable,
            "import { bookFormat } from \"@modules/domain/book/function\";\n\nexport function bookIsUnavailable(): boolean {\n\treturn bookFormat() !== \"book\";\n}\n",
        ),
    ]);
    assert_eq!(
        check(RULE, KINDS, &tree),
        [
            finding(
                "own-kind-folder",
                AVAILABLE,
                1,
                &format!(
                    "loads .. from inside its kind folder {KIND}; import bookFormat by its file {FORMAT}"
                ),
            ),
            finding(
                "own-kind-folder",
                unavailable,
                1,
                &format!(
                    "loads @modules/domain/book/function from inside its kind folder {KIND}; import bookFormat by its file {FORMAT}"
                ),
            ),
        ]
    );
}

#[test]
fn a_top_level_owner_named_like_a_kind_is_no_kind_folder() {
    let tree = [
        (
            "src/config/index.ts",
            "export { configServer } from \"./server.config\";\n",
        ),
        (
            "src/config/server.config.ts",
            "export const configServer = { port: 1 };\n",
        ),
        (
            "src/runtime/a.ts",
            "import { configServer } from \"@config/server.config\";\n\nexport const RUNTIME_PORT = configServer.port;\n",
        ),
        (
            "src/runtime/b.ts",
            "import { configServer } from \"@config\";\n\nexport const RUNTIME_OTHER_PORT = configServer.port;\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            "src/runtime/b.ts",
            1,
            "loads @config; import configServer by its file src/config/server.config.ts",
        )]
    );
}

#[test]
fn an_internal_child_has_its_own_barrel_and_the_internal_plane_is_no_such_child() {
    let tree = [
        (
            "src/common/function/format-name.function.ts",
            "export function commonFormatName(): string {\n\treturn \"name\";\n}\n",
        ),
        (
            "src/common/function/index.ts",
            "export { commonFormatName } from \"./format-name.function\";\n",
        ),
        (
            "src/common/function/internal/index.ts",
            "export { commonInternalTrim } from \"./trim.function\";\n",
        ),
        (
            "src/common/function/internal/trim.function.ts",
            "export function commonInternalTrim(value: string): string {\n\treturn value.trim();\n}\n",
        ),
        (
            "src/modules/internal/shelf/function/count.function.ts",
            "export function shelfCount(): number {\n\treturn 0;\n}\n",
        ),
        (
            "src/modules/internal/shelf/function/index.ts",
            "export { shelfCount } from \"./count.function\";\n",
        ),
        (
            "src/modules/public/report/service.ts",
            "import { shelfCount } from \"@modules/internal/shelf/function\";\n\nexport const REPORT_COUNT = shelfCount();\n",
        ),
        (
            "src/runtime/a.ts",
            "import { commonInternalTrim } from \"@common/function/internal\";\n\nexport const RUNTIME_NAME = commonInternalTrim(\" a \");\n",
        ),
        (
            "src/runtime/b.ts",
            "import { commonInternalTrim } from \"@common/function/internal/trim.function\";\n\nexport const RUNTIME_OTHER_NAME = commonInternalTrim(\" b \");\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            "src/runtime/b.ts",
            1,
            "loads @common/function/internal/trim.function; import commonInternalTrim through the internal barrel src/common/function/internal",
        )]
    );
}

#[test]
fn a_file_in_no_kind_folder_and_a_private_entrypoint_are_loaded_by_their_file() {
    let root = "src/infrastructure/storage/snapshot";
    let tree = [
        (
            "src/infrastructure/storage/snapshot/function/acquire.function.ts",
            "export function snapshotAcquire(): void {}\n",
        ),
        (
            "src/infrastructure/storage/snapshot/function/index.ts",
            "export { snapshotAcquire } from \"./acquire.function\";\n",
        ),
        (
            "src/infrastructure/storage/snapshot/function/seed.function.ts",
            "export function snapshotSeed(): void {}\n",
        ),
        (
            "src/infrastructure/storage/snapshot/index.ts",
            "export * from \"./function\";\nexport { SnapshotService } from \"./snapshot.service\";\n",
        ),
        (
            "src/infrastructure/storage/snapshot/snapshot.service.ts",
            "export class SnapshotService {}\n",
        ),
        (
            "src/runtime/a.ts",
            "import { SnapshotService } from \"@infrastructure/storage/snapshot/snapshot.service\";\n\nexport const RUNTIME_SERVICE = new SnapshotService();\n",
        ),
        (
            "src/runtime/b.ts",
            "import { SnapshotService } from \"@infrastructure/storage/snapshot\";\n\nexport const RUNTIME_OTHER_SERVICE = new SnapshotService();\n",
        ),
        (
            "src/runtime/c.ts",
            "import { snapshotSeed } from \"@infrastructure/storage/snapshot/function/seed.function\";\n\nsnapshotSeed();\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            "src/runtime/b.ts",
            1,
            &format!(
                "loads @infrastructure/storage/snapshot; import SnapshotService by its file {root}/snapshot.service.ts"
            ),
        )]
    );

    // Exported by its kind barrel, the seed is no private entrypoint any more.
    let mut exported = tree.to_vec();
    exported[1].1 = "export { snapshotAcquire } from \"./acquire.function\";\nexport { snapshotSeed } from \"./seed.function\";\n";
    assert_eq!(
        check(RULE, KINDS, &exported)[1],
        finding(
            "kind-barrel",
            "src/runtime/c.ts",
            1,
            &format!(
                "loads @infrastructure/storage/snapshot/function/seed.function; import snapshotSeed through the kind barrel {root}/function"
            ),
        )
    );
}

#[test]
fn a_name_a_kind_barrel_imports_and_exports_is_exported_through_it() {
    let tree = with(&[
        (
            "src/modules/domain/book/function/index.ts",
            "import { bookFormat } from \"./format.function\";\n\nexport { bookFormat };\nexport * from \"./is\";\n",
        ),
        (
            "src/modules/domain/author/service.ts",
            "import { bookFormat } from \"@modules/domain/book/function\";\n\nexport const AUTHOR_LABEL = bookFormat();\n",
        ),
    ]);
    assert_eq!(check(RULE, KINDS, &tree), []);
}

#[test]
fn a_relative_path_stays_inside_the_importers_resource() {
    let apply = "src/modules/domain/shelf/version/function/apply.function.ts";
    let tree = [
        (
            apply,
            "import type { IShelfVersionItem } from \"../interface\";\nimport { shelfVersionRuleRead } from \"../rule/function\";\n\nexport function shelfVersionApply(item: IShelfVersionItem): number {\n\treturn item.id.length + shelfVersionRuleRead();\n}\n",
        ),
        (
            "src/modules/domain/shelf/version/function/index.ts",
            "export { shelfVersionApply } from \"./apply.function\";\n",
        ),
        (
            "src/modules/domain/shelf/version/interface/index.ts",
            "export type { IShelfVersionItem } from \"./item.interface\";\n",
        ),
        (
            "src/modules/domain/shelf/version/interface/item.interface.ts",
            "export interface IShelfVersionItem {\n\treadonly id: string;\n}\n",
        ),
        (
            "src/modules/domain/shelf/version/rule/function/index.ts",
            "export { shelfVersionRuleRead } from \"./read.function\";\n",
        ),
        (
            "src/modules/domain/shelf/version/rule/function/read.function.ts",
            "export function shelfVersionRuleRead(): number {\n\treturn 1;\n}\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "resource-relative",
            apply,
            2,
            "loads ../rule/function, which leaves its resource src/modules/domain/shelf/version; use the owner alias",
        )]
    );

    // Through the owner alias, the nested resource passes.
    let mut aliased = tree.to_vec();
    aliased[0].1 = "import type { IShelfVersionItem } from \"../interface\";\nimport { shelfVersionRuleRead } from \"@modules/domain/shelf/version/rule/function\";\n\nexport function shelfVersionApply(item: IShelfVersionItem): number {\n\treturn item.id.length + shelfVersionRuleRead();\n}\n";
    assert_eq!(check(RULE, KINDS, &aliased), []);
}

#[test]
fn a_specifier_never_ends_in_index() {
    let service = "src/modules/domain/book/service.ts";
    let tree = with(&[(
        service,
        "import { bookFormat } from \"./function/index\";\nimport \"./function/index.js\";\n\nexport const BOOK_LABEL = bookFormat();\n",
    )]);
    assert_eq!(
        check(RULE, KINDS, &tree),
        [
            finding(
                "index-segment",
                service,
                1,
                "loads ./function/index; drop the /index segment",
            ),
            finding(
                "index-segment",
                service,
                2,
                "loads ./function/index.js; drop the /index segment",
            ),
        ]
    );
}

#[test]
fn one_import_per_module_and_a_deferred_module_is_not_loaded_statically() {
    let eager = "src/runtime/function/prepare-eager.function.ts";
    let start = "src/runtime/start.ts";
    let tree = with(&[
        (
            "src/infrastructure/metrics/function/index.ts",
            "export * from \"./is\";\n",
        ),
        (
            "src/infrastructure/metrics/function/is/full.function.ts",
            "export function metricsIsFull(): boolean {\n\treturn true;\n}\n",
        ),
        (
            "src/infrastructure/metrics/function/is/index.ts",
            "export { metricsIsFull } from \"./full.function\";\n",
        ),
        (
            "src/infrastructure/metrics/runtime/function/compose.function.ts",
            "export function metricsRuntimeCompose(): void {}\n",
        ),
        (
            "src/infrastructure/metrics/runtime/function/index.ts",
            "export { metricsRuntimeCompose } from \"./compose.function\";\n",
        ),
        (
            "src/modules/domain/book/constant/index.ts",
            "export { BOOK_LIMIT_CONSTANT } from \"./limit.constant\";\n",
        ),
        (
            "src/modules/domain/book/constant/limit.constant.ts",
            "export const BOOK_LIMIT_CONSTANT = { MAX: 3 } as const;\n",
        ),
        (
            "src/modules/domain/book/index.ts",
            "export * from \"./constant\";\nexport * from \"./function\";\n",
        ),
        (
            eager,
            "import { metricsIsFull } from \"@infrastructure/metrics/function\";\n\nexport async function runtimePrepareEager(): Promise<boolean> {\n\tconst { metricsIsFull: isFull } = await import(\"@infrastructure/metrics/function\");\n\treturn metricsIsFull() && isFull();\n}\n",
        ),
        (
            "src/runtime/function/prepare.function.ts",
            "import { metricsIsFull } from \"@infrastructure/metrics/function\";\n\nexport async function runtimePrepare(): Promise<void> {\n\tif (metricsIsFull()) {\n\t\tconst { metricsRuntimeCompose } = await import(\"@infrastructure/metrics/runtime/function\");\n\t\tmetricsRuntimeCompose();\n\t}\n}\n",
        ),
        (
            start,
            "export async function runtimeStart(): Promise<string> {\n\tconst { BOOK_LIMIT_CONSTANT, bookFormat } = await import(\"@modules/domain/book\");\n\treturn `${bookFormat()}:${BOOK_LIMIT_CONSTANT.MAX}`;\n}\n",
        ),
    ]);
    assert_eq!(
        check(RULE, KINDS, &tree),
        [
            finding(
                "dynamic-import",
                eager,
                4,
                &format!(
                    "defers @infrastructure/metrics/function with import() but already loads it statically; place the deferred module behind a barrel that {eager} does not load statically"
                ),
            ),
            finding(
                "kind-barrel",
                start,
                2,
                "loads @modules/domain/book; import BOOK_LIMIT_CONSTANT through the kind barrel src/modules/domain/book/constant and bookFormat through the kind barrel src/modules/domain/book/function",
            ),
            finding(
                "dynamic-import",
                start,
                2,
                "loads @modules/domain/book with import(); its names come from 2 modules (src/modules/domain/book/constant, src/modules/domain/book/function); use one import() per module",
            ),
        ]
    );

    // Re-exported by the barrel the file loads statically, the deferred module is loaded too.
    let mut reexported = tree.clone();
    for (path, text) in &mut reexported {
        if *path == "src/infrastructure/metrics/function/index.ts" {
            *text = "export * from \"./is\";\nexport * from \"../runtime/function\";\n";
        }
    }
    let prepare = "src/runtime/function/prepare.function.ts";
    assert!(check(RULE, KINDS, &reexported).contains(&finding(
        "dynamic-import",
        prepare,
        5,
        &format!(
            "defers @infrastructure/metrics/runtime/function with import() but already loads it statically; place the deferred module behind a barrel that {prepare} does not load statically"
        ),
    )));

    // An import() with both reasons gets one finding that gives both.
    let lazy = "src/runtime/lazy.ts";
    let mut both = tree.clone();
    both.push((
        lazy,
        "import { bookFormat } from \"@modules/domain/book/function\";\n\nexport async function runtimeLazy(): Promise<unknown> {\n\tconst { BOOK_LIMIT_CONSTANT, bookFormat: format } = await import(\"@modules/domain/book\");\n\treturn [bookFormat, format, BOOK_LIMIT_CONSTANT];\n}\n",
    ));
    let found: Vec<Row> = check(RULE, KINDS, &both)
        .into_iter()
        .filter(|(aspect, path, ..)| path == lazy && aspect == "dynamic-import")
        .collect();
    assert_eq!(
        found,
        [finding(
            "dynamic-import",
            lazy,
            4,
            &format!(
                "loads @modules/domain/book with import(); its names come from 2 modules (src/modules/domain/book/constant, src/modules/domain/book/function), and {lazy} already loads it statically; use one import() per module, and place each deferred module behind a barrel that {lazy} does not load statically"
            ),
        )]
    );
    assert_one_per_key(KINDS, &both);
}

#[test]
fn a_mock_names_a_module_the_form_makes_the_source_load() {
    let tests = "test/unit/modules/domain/author";
    let tree = with(&[
        (
            "src/modules/domain/author/service.ts",
            "import { bookIsAvailable } from \"@modules/domain/book/function\";\n\nexport function authorCanLend(): boolean {\n\treturn bookIsAvailable();\n}\n",
        ),
        (
            "test/unit/modules/domain/author/actual.test.ts",
            "import { vi } from \"vitest\";\n\nexport const BOOK_FUNCTION = await vi.importActual(\"@modules/domain/book/function\");\n",
        ),
        (
            "test/unit/modules/domain/author/file-mock.test.ts",
            "import { vi } from \"vitest\";\n\nvi.mock(\"@modules/domain/book/function/is/available.function\", () => ({ bookIsAvailable: vi.fn(() => true) }));\n",
        ),
        (
            "test/unit/modules/domain/author/import-mock.test.ts",
            "import { vi } from \"vitest\";\n\nexport const BOOK_IS = await vi.importMock(\"@modules/domain/book/function/is\");\n",
        ),
        (
            "test/unit/modules/domain/author/merge.test.ts",
            "import { vi } from \"vitest\";\n\nvi.mock(\"@modules/domain/book/function\", () => ({ bookFormat: vi.fn(() => \"book\") }));\nvi.mock(\"@modules/domain/book/function\", () => ({ bookIsAvailable: vi.fn(() => true) }));\n",
        ),
        (
            "test/unit/modules/domain/author/service.test.ts",
            "import { describe, expect, it, vi } from \"vitest\";\n\nimport { authorCanLend } from \"@modules/domain/author/service\";\n\nvi.mock(\"@modules/domain/book/function\", async (importOriginal) => ({\n\t...(await importOriginal<typeof import(\"@modules/domain/book/function\")>()),\n\tbookIsAvailable: vi.fn(() => true),\n}));\n\ndescribe(\"authorCanLend\", () => {\n\tit(\"lends an available book\", () => {\n\t\texpect(authorCanLend()).toBe(true);\n\t});\n});\n",
        ),
        (
            "test/unit/modules/domain/author/wide-mock.test.ts",
            "import { vi } from \"vitest\";\n\nvi.doMock(\"@modules/domain/book\", () => ({ bookIsAvailable: vi.fn(() => true) }));\n",
        ),
        (
            "test/unit/modules/domain/book/function/is/available.test.ts",
            "import { vi } from \"vitest\";\n\nvi.mock(\"@modules/domain/book/function/format.function\", () => ({ bookFormat: vi.fn(() => \"book\") }));\n",
        ),
    ]);
    let expected = [
        finding(
            "mock-target",
            &format!("{tests}/file-mock.test.ts"),
            3,
            &format!(
                "names @modules/domain/book/function/is/available.function in vi.mock, which no source file loads under the form; name the kind barrel {KIND}"
            ),
        ),
        finding(
            "mock-target",
            &format!("{tests}/import-mock.test.ts"),
            3,
            &format!(
                "names @modules/domain/book/function/is in vi.importMock, which no source file loads under the form; name the kind barrel {KIND}"
            ),
        ),
        finding(
            "mock-target",
            &format!("{tests}/merge.test.ts"),
            4,
            "names @modules/domain/book/function in vi.mock more than once; the test runner keeps only the last factory, so merge the factories",
        ),
        finding(
            "mock-target",
            &format!("{tests}/wide-mock.test.ts"),
            3,
            "names @modules/domain/book in vi.doMock, which no source file loads under the form; name the kind barrel or file that the source loads",
        ),
    ];
    assert_eq!(check(RULE, KINDS, &tree), expected);

    // Without the load of `format.function` by a file of its own kind folder, its mock names a
    // module no source file loads under the form.
    let mut unloaded = tree.clone();
    for (path, text) in &mut unloaded {
        if *path == AVAILABLE {
            *text = "export function bookIsAvailable(): boolean {\n\treturn true;\n}\n";
        }
    }
    assert!(check(RULE, KINDS, &unloaded).contains(&finding(
        "mock-target",
        "test/unit/modules/domain/book/function/is/available.test.ts",
        3,
        &format!(
            "names @modules/domain/book/function/format.function in vi.mock, which no source file loads under the form; name the kind barrel {KIND}"
        ),
    )));

    // A semantic subfolder's barrel that a file of the kind folder loads is still no module the
    // source loads under the form: that load is out of the form itself.
    let mut sibling = tree.clone();
    sibling.push((
        "src/modules/domain/book/function/shelve.function.ts",
        "import { bookIsAvailable } from \"./is\";\n\nexport const BOOK_SHELVED = bookIsAvailable();\n",
    ));
    assert!(check(RULE, KINDS, &sibling).contains(&expected[1]));

    // A hoisted call is any of `hoisted-mock-calls`: with none, two mocks of one module pass.
    let none = r#"{ "kinds": ["function"], "hoisted-mock-calls": [] }"#;
    assert_eq!(check(RULE, none, &tree).len(), 3);

    // A repeated call that also names a module out of the form gets one finding with both
    // reasons, on its own line; the first call keeps its own finding.
    let twice = format!("{tests}/twice.test.ts");
    let mut repeated = tree.clone();
    repeated.push((
        "test/unit/modules/domain/author/twice.test.ts",
        "import { vi } from \"vitest\";\n\nvi.mock(\"@modules/domain/book\", () => ({}));\nvi.mock(\"@modules/domain/book\", () => ({}));\n",
    ));
    let found: Vec<Row> = check(RULE, KINDS, &repeated)
        .into_iter()
        .filter(|(_, path, ..)| *path == twice)
        .collect();
    assert_eq!(
        found,
        [
            finding(
                "mock-target",
                &twice,
                3,
                "names @modules/domain/book in vi.mock, which no source file loads under the form; name the kind barrel or file that the source loads",
            ),
            finding(
                "mock-target",
                &twice,
                4,
                "names @modules/domain/book in vi.mock more than once, and no source file loads it under the form; name the kind barrel or file that the source loads, and merge the factories, since the test runner keeps only the last one",
            ),
        ]
    );
    assert_one_per_key(KINDS, &repeated);
}

#[test]
fn the_registry_and_an_entitys_enum_loads_are_left_to_their_rules() {
    let foreign = "src/modules/domain/author/entity/author.entity.ts";
    let tree = [
        (
            "src/migration/typeorm/generated/1700000000000-Baseline.ts",
            "export class Baseline1700000000000 {}\n",
        ),
        (
            "src/migration/typeorm/generated/index.ts",
            "export { Baseline1700000000000 } from \"./1700000000000-Baseline\";\n",
        ),
        (
            "src/migration/typeorm/migrations.ts",
            "import { Baseline1700000000000 } from \"./generated\";\n\nexport const TYPEORM_MIGRATIONS_CONSTANT = [Baseline1700000000000];\n",
        ),
        (
            "src/modules/domain/book/entity/book.entity.ts",
            "import { EBookState } from \"../enum\";\nimport { EBookState as EState } from \"@modules/domain/book/enum/index\";\n\nexport class BookEntity {\n\tstate: EBookState = EState.ACTIVE;\n}\n",
        ),
        (
            "src/modules/domain/book/entity/index.ts",
            "export { BookEntity } from \"./book.entity\";\n",
        ),
        (
            "src/modules/domain/book/enum/index.ts",
            "export { EBookState } from \"./state.enum\";\n",
        ),
        (
            "src/modules/domain/book/enum/state.enum.ts",
            "export enum EBookState {\n\tACTIVE = \"active\",\n}\n",
        ),
        (
            foreign,
            "import { EBookState } from \"@modules/domain/book/enum/state.enum\";\nimport { EBookState as EOther } from \"@modules/domain/book/enum\";\n\nexport class AuthorEntity {\n\tstate: EBookState = EOther.ACTIVE;\n}\n",
        ),
        (
            "src/modules/domain/author/entity/index.ts",
            "export { AuthorEntity } from \"./author.entity\";\n",
        ),
    ];
    assert_eq!(check(RULE, KINDS, &tree), []);
    // `nestjs/import-aliases` reports the enum file and the own enum barrel through `/index`, and
    // this rule reports neither again, under any aspect.
    let own = "src/modules/domain/book/entity/book.entity.ts";
    let aliases = check("import-aliases", "{}", &tree);
    assert_eq!(aliases.len(), 2, "{aliases:?}");
    for expected in [
        row(
            "entity-enum-barrel",
            foreign,
            None,
            &format!(
                "{foreign} loads the enum file @modules/domain/book/enum/state.enum; import the enum through its owner's enum barrel"
            ),
        ),
        row(
            "entity-enum-barrel",
            own,
            None,
            &format!(
                "{own} loads its own resource's enum barrel through @modules/domain/book/enum/index; import it as ../enum"
            ),
        ),
    ] {
        assert!(aliases.contains(&expected), "{aliases:?}");
    }
    // With another enum folder, the load through `/index` is this rule's.
    let enums = r#"{ "kinds": ["entity", "enum"], "enum-folder": "enums" }"#;
    assert_eq!(
        check(RULE, enums, &tree),
        [finding(
            "index-segment",
            own,
            2,
            "loads @modules/domain/book/enum/index; drop the /index segment",
        )]
    );
    // Outside an entity folder, the enum file is this rule's.
    let mut moved = tree.to_vec();
    moved[7].0 = "src/modules/domain/author/author.service.ts";
    assert_eq!(
        check(RULE, KINDS, &moved),
        [finding(
            "kind-barrel",
            "src/modules/domain/author/author.service.ts",
            1,
            "loads @modules/domain/book/enum/state.enum; import EBookState through the kind barrel src/modules/domain/book/enum",
        )]
    );
    // Without the registry exemption, its import of the migrations is a load by a barrel.
    let other = r#"{ "kinds": ["entity", "enum"], "registry": "src/migration/typeorm/other.ts" }"#;
    assert_eq!(
        check(RULE, other, &tree),
        [finding(
            "kind-barrel",
            "src/migration/typeorm/migrations.ts",
            1,
            "loads ./generated; import Baseline1700000000000 by its file src/migration/typeorm/generated/1700000000000-Baseline.ts",
        )]
    );
}

#[test]
fn a_kind_named_folder_whose_subfolders_hold_other_files_is_no_kind_folder() {
    let tree = [
        (
            "src/modules/domain/shelf/module.ts",
            "import { ShelfDtoExportModule } from \"./dto/export/module\";\n\nexport class ShelfModule {\n\treadonly imports = [ShelfDtoExportModule];\n}\n",
        ),
        (
            "src/modules/domain/shelf/dto/export/index.ts",
            "export { ShelfDtoExportModule } from \"./module\";\nexport { ShelfDtoExportService } from \"./service\";\n",
        ),
        (
            "src/modules/domain/shelf/dto/export/module.ts",
            "export class ShelfDtoExportModule {}\n",
        ),
        (
            "src/modules/domain/shelf/dto/export/service.ts",
            "export class ShelfDtoExportService {}\n",
        ),
        (
            "src/modules/domain/shelf/dto/index.ts",
            "export * from \"./export\";\n",
        ),
        (
            "src/modules/domain/author/module.ts",
            "import { ShelfDtoExportModule } from \"@modules/domain/shelf/dto\";\n\nexport class AuthorModule {\n\treadonly imports = [ShelfDtoExportModule];\n}\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            "src/modules/domain/author/module.ts",
            1,
            "loads @modules/domain/shelf/dto; import ShelfDtoExportModule by its file src/modules/domain/shelf/dto/export/module.ts",
        )]
    );
}

#[test]
fn a_barrel_re_exports_a_semantic_subfolder_only_through_its_kind_barrel() {
    let barrel = "src/modules/integration/shop/command/index.ts";
    let tree = [
        (
            "src/modules/integration/shop/command/function/index.ts",
            "export * from \"./mapper\";\n",
        ),
        (
            "src/modules/integration/shop/command/function/mapper/create.function.ts",
            "export function shopCommandMapCreate(): void {}\n",
        ),
        (
            "src/modules/integration/shop/command/function/mapper/index.ts",
            "export { shopCommandMapCreate } from \"./create.function\";\n",
        ),
        (
            barrel,
            "export * from \"./function\";\nexport { shopCommandMapCreate } from \"./function/mapper\";\n",
        ),
        (
            "src/modules/integration/shop/command/parser/index.ts",
            "export { shopCommandMapCreate } from \"../function\";\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            barrel,
            2,
            "loads ./function/mapper; import shopCommandMapCreate through the kind barrel src/modules/integration/shop/command/function",
        )]
    );
    // The named re-export from the kind barrel is the form.
    let mut named = tree.to_vec();
    named[3].1 =
        "export * from \"./function\";\nexport { shopCommandMapCreate } from \"./function\";\n";
    assert_eq!(check(RULE, KINDS, &named), []);
}

#[test]
fn a_name_the_kind_barrel_leaves_out_waits_for_the_barrel() {
    let raw = "src/modules/domain/author/raw.service.ts";
    let tree = with(&[
        (
            raw,
            "import { bookFormatRaw } from \"@modules/domain/book/function/format.function\";\n\nexport const AUTHOR_RAW_LABEL = bookFormatRaw();\n",
        ),
        (
            FORMAT,
            "export function bookFormat(): string {\n\treturn bookFormatRaw().trim();\n}\n\nexport function bookFormatRaw(): string {\n\treturn \" book \";\n}\n",
        ),
        (
            "src/modules/domain/book/function/is/raw.function.ts",
            "import { bookFormatRaw } from \"../format.function\";\n\nexport function bookIsRaw(): boolean {\n\treturn bookFormatRaw() !== \"\";\n}\n",
        ),
    ]);
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            raw,
            1,
            &format!(
                "loads @modules/domain/book/function/format.function; import bookFormatRaw through the kind barrel {KIND} once it exports them"
            ),
        )]
    );
}

#[test]
fn inside_its_kind_folder_an_importer_uses_the_owner_alias_only_beyond_one_parent() {
    let known = "src/modules/domain/book/function/is/known.function.ts";
    let tree = with(&[
        (
            "src/modules/domain/book/function/is/deep/archived.function.ts",
            "import { bookFormat } from \"@modules/domain/book/function/format.function\";\n\nexport function bookIsArchived(): boolean {\n\treturn bookFormat() === \"archived\";\n}\n",
        ),
        (
            known,
            "import { bookFormat } from \"@modules/domain/book/function/format.function\";\n\nexport function bookIsKnown(): boolean {\n\treturn bookFormat() !== \"\";\n}\n",
        ),
    ]);
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "own-kind-folder",
            known,
            1,
            &format!(
                "loads @modules/domain/book/function/format.function by an owner alias; a relative path with at most one ../ reaches {FORMAT} in its own kind folder"
            ),
        )]
    );
}

#[test]
fn a_namespace_load_takes_every_name_its_module_exports() {
    let barrel = "src/modules/domain/author/index.ts";
    let wide = "src/modules/domain/author/wide.service.ts";
    let all = "src/modules/domain/book/function/is/all.function.ts";
    let tree = with(&[
        (
            barrel,
            "export * from \"@modules/domain/book/function/is\";\nexport { AUTHOR_LABEL } from \"./service\";\n",
        ),
        (
            "src/modules/domain/author/service.ts",
            "import * as bookFunction from \"@modules/domain/book/function\";\n\nexport const AUTHOR_LABEL = bookFunction.bookFormat();\n",
        ),
        (
            wide,
            "import * as book from \"@modules/domain/book\";\n\nexport const AUTHOR_WIDE_LABEL = book.bookFormat();\n",
        ),
        (
            all,
            "import * as bookFunction from \"..\";\n\nexport function bookIsAll(): boolean {\n\treturn bookFunction.bookIsAvailable() && bookFunction.bookFormat() === \"book\";\n}\n",
        ),
    ]);
    assert_eq!(
        check(RULE, KINDS, &tree),
        [
            finding(
                "kind-barrel",
                barrel,
                1,
                &format!(
                    "loads @modules/domain/book/function/is; import bookIsAvailable through the kind barrel {KIND}"
                ),
            ),
            finding(
                "kind-barrel",
                wide,
                1,
                &format!(
                    "loads @modules/domain/book; import bookFormat, bookIsAvailable through the kind barrel {KIND}"
                ),
            ),
            finding(
                "own-kind-folder",
                all,
                1,
                &format!(
                    "loads .. from inside its kind folder {KIND}; import bookFormat by its file {FORMAT} and bookIsAvailable by its file {AVAILABLE}"
                ),
            ),
        ]
    );
}

#[test]
fn every_load_form_takes_its_names() {
    let forms = "src/modules/domain/author/forms.ts";
    let tree = with(&[(
        forms,
        "import Format = require(\"@modules/domain/book/function/format.function\");\nexport * as bookIs from \"@modules/domain/book/function/is\";\nexport const FORMAT = require(\"@modules/domain/book/function/format.function\");\ntype TFormat = typeof import(\"@modules/domain/book/function/format.function\").bookFormat;\nexport type { TFormat };\nexport const LABEL = Format.bookFormat();\n",
    )]);
    let through = |line: u32, specifier: &str, names: &str| {
        finding(
            "kind-barrel",
            forms,
            line,
            &format!("loads {specifier}; import {names} through the kind barrel {KIND}"),
        )
    };
    assert_eq!(
        check(RULE, KINDS, &tree),
        [
            through(
                1,
                "@modules/domain/book/function/format.function",
                "bookFormat"
            ),
            through(2, "@modules/domain/book/function/is", "bookIsAvailable"),
            through(
                3,
                "@modules/domain/book/function/format.function",
                "bookFormat"
            ),
            through(
                4,
                "@modules/domain/book/function/format.function",
                "bookFormat"
            ),
        ]
    );
}

#[test]
fn declaration_files_count_for_a_kind_folder_and_resolve() {
    let reader = "src/modules/domain/author/reader.ts";
    let tree = [
        (
            "src/modules/domain/book/interface/shape.interface.d.ts",
            "export interface IBookShape {\n\treadonly id: string;\n}\n",
        ),
        (
            "src/modules/domain/book/interface/index.ts",
            "export type { IBookShape } from \"./shape.interface\";\n",
        ),
        (
            "src/modules/domain/book/dto/notes.d.ts",
            "export interface IBookNotes {\n\treadonly text: string;\n}\n",
        ),
        (
            "src/modules/domain/book/dto/index.ts",
            "export type { IBookNotes } from \"./notes\";\n",
        ),
        (
            reader,
            "import type { IBookShape } from \"@modules/domain/book/interface/shape.interface\";\nimport type { IBookNotes } from \"@modules/domain/book/dto/notes\";\n\nexport type TReader = [IBookShape, IBookNotes];\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            reader,
            1,
            "loads @modules/domain/book/interface/shape.interface; import IBookShape through the kind barrel src/modules/domain/book/interface",
        )]
    );
}

#[test]
fn options_that_cannot_hold_are_errors_that_name_the_key() {
    let tree = base();
    for (options, expected) in [
        (r#"{ "kinds": [] }"#, "option `kinds` is empty"),
        (
            r#"{ "kinds": ["dto", "dto"] }"#,
            "option `kinds[1]` is \"dto\", which the list already holds",
        ),
        (
            r#"{ "kinds": ["dto/x"] }"#,
            "option `kinds[0]` is \"dto/x\"",
        ),
        (
            r#"{ "hoisted-mock-calls": ["jest.mock"] }"#,
            "option `hoisted-mock-calls[0]` is \"jest.mock\", which `mock-calls` does not list",
        ),
        (
            r#"{ "hoisted-mock-calls": ["vi.mock", "vi.mock"] }"#,
            "option `hoisted-mock-calls[1]` is \"vi.mock\", which the list already holds",
        ),
        (
            r#"{ "mock-calls": ["vi.mock", "vi.mock"] }"#,
            "option `mock-calls[1]` is \"vi.mock\", which the list already holds",
        ),
        (
            r#"{ "mock-calls": ["vi..mock"], "hoisted-mock-calls": [] }"#,
            "option `mock-calls[0]` is \"vi..mock\", which is not a dotted name",
        ),
        (
            r#"{ "enum-folder": "enum/x" }"#,
            "option `enum-folder` is \"enum/x\"",
        ),
        (r#"{ "kind": ["dto"] }"#, "`kind`"),
    ] {
        let message = error(RULE, options, &tree);
        assert!(message.contains(expected), "{options}: {message}");
    }
    assert!(
        host::run(
            RULE,
            "{}",
            r#"{ "layout": { "exclusive-kind-folders": [] } }"#,
            &tree
        )
        .expect_err("the rule fails")
        .message()
        .contains("layout.exclusive-kind-folders is empty")
    );
}

#[test]
fn the_groups_of_a_message_follow_the_paths_of_their_modules() {
    let reader = "src/modules/domain/author/reader.ts";
    let tree = [
        (
            "src/modules/domain/shelf/a.service.ts",
            "export class ShelfAService {}\n",
        ),
        (
            "src/modules/domain/shelf/constant/index.ts",
            "export { SHELF_SIZE_CONSTANT } from \"./size.constant\";\n",
        ),
        (
            "src/modules/domain/shelf/constant/size.constant.ts",
            "const SHELF_SIZE_CONSTANT = 3;\n\nexport { SHELF_SIZE_CONSTANT };\n",
        ),
        (
            "src/modules/domain/shelf/index.ts",
            "export * from \"./a.service\";\nexport * from \"./constant\";\nexport * from \"./z.service\";\n",
        ),
        (
            "src/modules/domain/shelf/z.service.ts",
            "export class ShelfZService {}\n",
        ),
        (
            reader,
            "import { ShelfZService, SHELF_SIZE_CONSTANT, ShelfAService } from \"@modules/domain/shelf\";\n\nexport const READER = [ShelfAService, SHELF_SIZE_CONSTANT, ShelfZService];\n",
        ),
    ];
    assert_eq!(
        check(RULE, KINDS, &tree),
        [finding(
            "kind-barrel",
            reader,
            1,
            "loads @modules/domain/shelf; import ShelfAService by its file src/modules/domain/shelf/a.service.ts and SHELF_SIZE_CONSTANT through the kind barrel src/modules/domain/shelf/constant and ShelfZService by its file src/modules/domain/shelf/z.service.ts",
        )]
    );
}

#[test]
fn the_groups_follow_the_paths_of_their_modules_whatever_the_order_of_kinds() {
    let service = "src/modules/domain/author/service.ts";
    let tree = with(&[
        (
            "src/modules/domain/book/dto/book.dto.ts",
            "export class BookDto {}\n",
        ),
        (
            "src/modules/domain/book/dto/index.ts",
            "export { BookDto } from \"./book.dto\";\n",
        ),
        (
            "src/modules/domain/book/index.ts",
            "export * from \"./function\";\nexport * from \"./dto\";\n",
        ),
        (
            service,
            "import { bookFormat, BookDto } from \"@modules/domain/book\";\n\nexport const AUTHOR = [bookFormat, BookDto];\n",
        ),
    ]);
    let expected = [finding(
        "kind-barrel",
        service,
        1,
        &format!(
            "loads @modules/domain/book; import BookDto through the kind barrel src/modules/domain/book/dto and bookFormat through the kind barrel {KIND}"
        ),
    )];
    // `kinds` not in alphabetical order: the groups still follow the paths.
    assert_eq!(
        check(RULE, r#"{ "kinds": ["function", "dto"] }"#, &tree),
        expected
    );
    assert_eq!(
        check(RULE, r#"{ "kinds": ["dto", "function"] }"#, &tree),
        expected
    );
}

//! The TypeScript library reads the tracked files of one source: file set, parse cache, lines and
//! module resolution with tsconfig `paths`.

use std::rc::Rc;

use gate_typescript::oxc_ast::ast::Statement;
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::{
    Allocator, MemorySource, Settings, Source, TypeScript, parsed_on_this_thread,
};

const TSCONFIG: &str = r#"{
 // comments and trailing commas are allowed, as in the TypeScript compiler
 "compilerOptions": {
  "paths": { "@modules/*": ["./src/modules/*"], "@shared/*": ["./src/shared/*"], },
 },
}
"#;

const SOURCES: &[(&str, &str)] = &[
    ("tsconfig.json", TSCONFIG),
    ("src/app.ts", "import { a } from \"./a\";\n"),
    ("src/a.ts", "export const a = 1;\n"),
    ("src/a.js", "exports.a = 1;\n"),
    ("src/b.ts", "export const b = 2;\n"),
    ("src/types.d.ts", "declare const c: number;\n"),
    ("src/view.tsx", "export const v = 1;\n"),
    ("src/legacy.mts", "export {};\n"),
    ("src/legacy.cts", "export {};\n"),
    ("src/script.mjs", "export {};\n"),
    ("src/modules/user/index.ts", "export {};\n"),
    (
        "src/modules/user/service.ts",
        "export class UserService {}\n",
    ),
    ("src/shared/only.js", "module.exports = {};\n"),
    (
        "src/package/package.json",
        "{ \"types\": \"./lib/entry.d.ts\" }\n",
    ),
    ("src/package/lib/entry.d.ts", "export {};\n"),
    ("node_modules/vendored/index.d.ts", "export {};\n"),
    ("src/Upper.TS", "export {};\n"),
    ("README.md", "# Fixture\n"),
];

/// The bundler-mode cases: a root `package.json` with `imports` and `exports`, and files whose
/// extensions the compiler replaces or never loads.
const BUNDLER: &[(&str, &str)] = &[
    ("tsconfig.json", TSCONFIG),
    (
        "package.json",
        r##"{ "name": "root", "imports": { "#internal": { "types": "./src/internal.ts", "default": "./src/missing.js" }, "#decl.d.ts": "./src/internal.ts" }, "exports": { ".": { "require": "./src/missing.js", "import": "./src/entry.ts" }, "./sub": "./src/sub.ts" } }"##,
    ),
    ("src/app.ts", "export {};\n"),
    ("src/internal.ts", "export {};\n"),
    ("src/entry.ts", "export {};\n"),
    ("src/sub.ts", "export {};\n"),
    ("src/view.jsx", "export {};\n"),
    ("src/worker.mts", "export {};\n"),
    ("src/legacy.cts", "export {};\n"),
    ("src/data.json", "{}\n"),
    ("src/both", "plain file without an extension\n"),
    ("src/both.ts", "export {};\n"),
    ("src/bare", "plain file without an extension\n"),
];

#[test]
fn the_file_set_is_every_tracked_typescript_file_in_path_order() {
    let source = MemorySource::new(SOURCES.iter().copied());
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);

    assert_eq!(
        typescript.files(),
        [
            "node_modules/vendored/index.d.ts",
            "src/a.ts",
            "src/app.ts",
            "src/b.ts",
            "src/legacy.cts",
            "src/legacy.mts",
            "src/modules/user/index.ts",
            "src/modules/user/service.ts",
            "src/package/lib/entry.d.ts",
            "src/types.d.ts",
            "src/view.tsx",
        ]
    );
    assert_eq!(typescript.files_under("").count(), typescript.files().len());
    assert_eq!(
        typescript.files_under("src/modules").collect::<Vec<_>>(),
        ["src/modules/user/index.ts", "src/modules/user/service.ts"]
    );
}

#[test]
fn a_tracked_file_is_parsed_once_per_run_and_reports_lines() {
    let source = "// leading comment\n\r\nexport class First {}\r\nexport class Second {}\n";
    let source = MemorySource::new([("src/classes.ts", source)]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let first = typescript
        .parse("src/classes.ts")
        .expect("parses")
        .expect("tracked");
    let again = typescript
        .parse("src/classes.ts")
        .expect("parses")
        .expect("tracked");
    let lines: Vec<u32> = first
        .program()
        .body
        .iter()
        .map(|statement| {
            assert!(
                matches!(statement, Statement::ExportDeclaration(_)),
                "{statement:?}"
            );
            first.line(statement.span().start)
        })
        .collect();

    assert!(Rc::ptr_eq(&first, &again));
    assert_eq!(lines, [3, 4]);
    assert!(first.errors().is_empty() && !first.fatal());
    assert!(typescript.parse("src/missing.ts").expect("reads").is_none());
}

#[test]
fn a_file_oxc_cannot_parse_has_an_empty_program_and_its_error_line() {
    let source = "export const a = 1;\nconst = ;\nexport const b = 2;\n";
    let source = MemorySource::new([("src/broken.ts", source)]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let file = typescript
        .parse("src/broken.ts")
        .expect("parses")
        .expect("tracked");

    assert_eq!(
        file.errors()
            .first()
            .map(|error| (error.line, error.column)),
        Some((Some(2), Some(7)))
    );
    assert!(file.fatal() && file.program().body.is_empty());
}

#[test]
fn a_file_that_is_not_utf8_text_is_a_fatal_file_without_a_line() {
    let source = MemorySource::new([(
        "test/media/clip.ts",
        [0x47_u8, 0x40, 0x11, 0x10, 0xff, 0xfe, 0x00],
    )]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let file = typescript
        .parse("test/media/clip.ts")
        .expect("reads")
        .expect("tracked");

    assert!(file.fatal() && file.program().body.is_empty());
    assert_eq!(
        file.errors()
            .iter()
            .map(|error| (error.line, error.message.as_str()))
            .collect::<Vec<_>>(),
        [(None, "the file is not UTF-8 text")]
    );
}

#[test]
fn a_path_without_a_script_extension_is_an_error() {
    let source = MemorySource::new([("README.md", "# Fixture\n")]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);

    assert!(typescript.parse("README.md").is_err());
}

#[test]
fn the_semantic_model_binds_references_to_their_declarations() {
    let source = "import { a } from \"./a\";\nconst b = a;\nexport const c = b + a;\n";
    let source = MemorySource::new([("src/use.ts", source)]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let file = typescript
        .parse("src/use.ts")
        .expect("parses")
        .expect("tracked");
    let scoping = file.semantic().scoping();
    let symbol = scoping
        .find_binding(scoping.root_scope_id(), "a".into())
        .expect("a is bound");
    let lines: Vec<u32> = scoping
        .get_resolved_references(symbol)
        .map(|reference| {
            file.line(
                file.semantic()
                    .nodes()
                    .get_node(reference.node_id())
                    .span()
                    .start,
            )
        })
        .collect();

    assert_eq!(lines, [2, 3]);
}

#[test]
fn specifiers_resolve_to_tracked_files_as_the_typescript_compiler_resolves_them() {
    let source = MemorySource::new(SOURCES.iter().copied());
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let resolve = |specifier: &str| {
        typescript
            .resolve("src/app.ts", specifier)
            .expect("resolves")
    };

    // `.ts` wins over `.js`; a `.js` specifier names its TypeScript source.
    assert_eq!(resolve("./a").as_deref(), Some("src/a.ts"));
    assert_eq!(resolve("./b.js").as_deref(), Some("src/b.ts"));
    assert_eq!(resolve("./types").as_deref(), Some("src/types.d.ts"));
    // A declaration specifier tries the source extensions first, as the compiler drops `.d.ts`.
    assert_eq!(resolve("./a.d.ts").as_deref(), Some("src/a.ts"));
    assert_eq!(resolve("./types.d.ts").as_deref(), Some("src/types.d.ts"));
    assert_eq!(resolve("./legacy.d.mts").as_deref(), Some("src/legacy.mts"));
    assert_eq!(
        resolve("@modules/user/service.d.ts").as_deref(),
        Some("src/modules/user/service.ts")
    );
    assert_eq!(resolve("./view").as_deref(), Some("src/view.tsx"));
    // tsconfig paths, directory index and package typings.
    assert_eq!(
        resolve("@modules/user").as_deref(),
        Some("src/modules/user/index.ts")
    );
    assert_eq!(
        resolve("@modules/user/service").as_deref(),
        Some("src/modules/user/service.ts")
    );
    assert_eq!(
        resolve("@modules/user/../user/service.js").as_deref(),
        Some("src/modules/user/service.ts")
    );
    assert_eq!(
        resolve("@shared/only").as_deref(),
        Some("src/shared/only.js")
    );
    assert_eq!(
        resolve("./package").as_deref(),
        Some("src/package/lib/entry.d.ts")
    );
    // Untracked files, packages (tracked or not), built-ins, paths above the root and a
    // differently cased path resolve to nothing.
    assert_eq!(resolve("./untracked"), None);
    assert_eq!(resolve("@scope/package"), None);
    assert_eq!(resolve("vendored"), None);
    assert_eq!(resolve("node:path"), None);
    assert_eq!(resolve("../../outside"), None);
    assert_eq!(resolve("./A"), None);
}

#[test]
fn bundler_mode_reads_package_maps_and_replaces_known_extensions() {
    let source = MemorySource::new(BUNDLER.iter().copied());
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let resolve = |specifier: &str| {
        typescript
            .resolve("src/app.ts", specifier)
            .expect("resolves")
    };

    // The package's own `imports` and self-referencing `exports`, with the `import`, `types` and
    // `default` conditions and never `require`.
    assert_eq!(resolve("#internal").as_deref(), Some("src/internal.ts"));
    // A key that ends in a declaration extension matches as written once its source form fails.
    assert_eq!(resolve("#decl.d.ts").as_deref(), Some("src/internal.ts"));
    assert_eq!(resolve("root").as_deref(), Some("src/entry.ts"));
    assert_eq!(resolve("root/sub").as_deref(), Some("src/sub.ts"));
    // A known extension is replaced by the compiler's candidates.
    assert_eq!(resolve("./view.js").as_deref(), Some("src/view.jsx"));
    assert_eq!(resolve("./worker.mjs").as_deref(), Some("src/worker.mts"));
    assert_eq!(resolve("./legacy.cjs").as_deref(), Some("src/legacy.cts"));
    assert_eq!(resolve("./data.json").as_deref(), Some("src/data.json"));
    // A file without a script extension is never loaded as written.
    assert_eq!(resolve("./both").as_deref(), Some("src/both.ts"));
    assert_eq!(resolve("./bare"), None);
}

#[test]
fn resolution_reads_the_tsconfig_of_the_source() {
    let shared =
        "{ \"compilerOptions\": { \"paths\": { \"@modules/*\": [\"./src/shared/*\"] } } }\n";
    let original = MemorySource::new(SOURCES.iter().copied());
    let changed = MemorySource::new(SOURCES.iter().map(|&(path, contents)| {
        (
            path,
            if path == "tsconfig.json" {
                shared
            } else {
                contents
            },
        )
    }));
    let allocator = Allocator::default();

    assert_eq!(
        TypeScript::new(&original, Settings::default(), &allocator)
            .resolve("src/app.ts", "@modules/user/service")
            .expect("resolves")
            .as_deref(),
        Some("src/modules/user/service.ts")
    );
    assert_eq!(
        TypeScript::new(&changed, Settings::default(), &allocator)
            .resolve("src/app.ts", "@modules/only")
            .expect("resolves")
            .as_deref(),
        Some("src/shared/only.js")
    );
}

#[test]
fn the_tsconfig_setting_names_the_tsconfig_the_resolver_reads() {
    let source = MemorySource::new([
        (
            "tsconfig.base.json",
            "{ \"compilerOptions\": { \"paths\": { \"@lib/*\": [\"./lib/*\"] } } }\n",
        ),
        ("tsconfig.json", "{ \"compilerOptions\": \n"),
        ("app.ts", "export {};\n"),
        ("lib/util.ts", "export {};\n"),
    ]);
    let settings = Settings {
        tsconfig: "tsconfig.base.json".to_owned(),
        ..Settings::default()
    };
    let allocator = Allocator::default();

    assert_eq!(
        TypeScript::new(&source, settings, &allocator)
            .resolve("app.ts", "@lib/util")
            .expect("resolves")
            .as_deref(),
        Some("lib/util.ts")
    );
    // The default names the broken tsconfig.json, which fails closed.
    assert!(
        TypeScript::new(&source, Settings::default(), &allocator)
            .resolve("app.ts", "@lib/util")
            .is_err()
    );
}

#[test]
fn an_unreadable_tsconfig_is_an_error_and_a_missing_one_disables_paths() {
    let broken = MemorySource::new([
        ("tsconfig.json", "{ \"compilerOptions\": \n"),
        ("src/app.ts", "export {};\n"),
    ]);
    let missing = MemorySource::new([
        ("src/app.ts", "export {};\n"),
        ("src/modules/user.ts", "export {};\n"),
    ]);
    let allocator = Allocator::default();

    assert!(
        TypeScript::new(&broken, Settings::default(), &allocator)
            .resolve("src/app.ts", "@modules/user")
            .is_err()
    );
    assert_eq!(
        TypeScript::new(&missing, Settings::default(), &allocator)
            .resolve("src/app.ts", "@modules/user")
            .expect("resolves"),
        None
    );
}

#[test]
fn a_specifier_that_leaves_the_project_resolves_to_nothing() {
    let source = MemorySource::new([
        (
            "tsconfig.json",
            "{ \"compilerOptions\": { \"paths\": { \"@up/*\": [\"../shared/*\"] } } }\n",
        ),
        ("src/app.ts", "export {};\n"),
        ("src/a.ts", "export {};\n"),
        ("shared/util.ts", "export {};\n"),
    ]);
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let resolve = |specifier| {
        typescript
            .resolve("src/app.ts", specifier)
            .expect("resolves")
    };

    assert_eq!(resolve("./a").as_deref(), Some("src/a.ts"));
    assert_eq!(resolve("../shared/util").as_deref(), Some("shared/util.ts"));
    assert_eq!(resolve("../../src/a"), None);
    assert_eq!(resolve("../../../../../src/a"), None);
    assert_eq!(resolve("@up/util"), None);
}

#[test]
fn a_configured_tsconfig_that_is_not_tracked_is_an_error() {
    let source = MemorySource::new([("src/app.ts", "export {};\n")]);
    let settings = Settings {
        tsconfig: "tsconfig.app.json".to_owned(),
        ..Settings::default()
    };
    let allocator = Allocator::default();
    let error = TypeScript::new(&source, settings, &allocator)
        .resolve("src/app.ts", "./a")
        .expect_err("fails");

    assert!(error.message().contains("tsconfig.app.json"), "{error}");
}

#[test]
fn an_error_names_paths_relative_to_the_project() {
    let source = MemorySource::new([
        ("tsconfig.json", "{ \"compilerOptions\": \n"),
        ("src/app.ts", "export {};\n"),
    ]);
    let allocator = Allocator::default();
    let error = TypeScript::new(&source, Settings::default(), &allocator)
        .resolve("src/app.ts", "@modules/user")
        .expect_err("fails");

    assert!(!error.message().contains("/gate/project"), "{error}");
}

#[test]
fn two_views_with_different_tsconfigs_resolve_apart_in_either_order() {
    let source = MemorySource::new([
        (
            "tsconfig.json",
            "{ \"compilerOptions\": { \"paths\": { \"@lib/*\": [\"./first/*\"] } } }\n",
        ),
        (
            "tsconfig.other.json",
            "{ \"compilerOptions\": { \"paths\": { \"@lib/*\": [\"./second/*\"] } } }\n",
        ),
        ("app.ts", "export {};\n"),
        ("first/util.ts", "export {};\n"),
        ("second/util.ts", "export {};\n"),
    ]);
    let other = Settings {
        tsconfig: "tsconfig.other.json".to_owned(),
        ..Settings::default()
    };
    let allocator = Allocator::default();
    let default = TypeScript::new(&source, Settings::default(), &allocator);
    let named = TypeScript::new(&source, other, &allocator);
    let resolve =
        |typescript: &TypeScript<'_>| typescript.resolve("app.ts", "@lib/util").expect("resolves");

    // Each view keeps its own answers: the first view's answer never serves the second.
    for _ in 0..2 {
        assert_eq!(resolve(&default).as_deref(), Some("first/util.ts"));
        assert_eq!(resolve(&named).as_deref(), Some("second/util.ts"));
    }
    assert_eq!(resolve(&named).as_deref(), Some("second/util.ts"));
    assert_eq!(resolve(&default).as_deref(), Some("first/util.ts"));
}

/// A source that counts what a view asks it for.
struct Counting {
    files: MemorySource,
    reads: std::cell::Cell<usize>,
    reads_for_parse: std::cell::Cell<usize>,
}

impl Source for Counting {
    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        self.files.paths()
    }

    fn bytes(&self, path: &str) -> gate_typescript::Result<Option<Rc<[u8]>>> {
        self.reads.set(self.reads.get() + 1);
        self.files.bytes(path)
    }

    fn bytes_for_parse(&self, path: &str) -> gate_typescript::Result<Option<Rc<[u8]>>> {
        self.reads_for_parse.set(self.reads_for_parse.get() + 1);
        self.files.bytes(path)
    }

    fn contains(&self, path: &str) -> bool {
        self.files.contains(path)
    }
}

#[test]
fn a_view_reads_each_file_for_its_parse_once_and_counts_the_parse() {
    let source = Counting {
        files: MemorySource::new(SOURCES.iter().copied()),
        reads: std::cell::Cell::new(0),
        reads_for_parse: std::cell::Cell::new(0),
    };
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let before = parsed_on_this_thread();

    assert!(typescript.parsed("src/app.ts").is_none());
    let first = typescript
        .parse("src/app.ts")
        .expect("parses")
        .expect("tracked");
    let again = typescript
        .parse("src/app.ts")
        .expect("parses")
        .expect("tracked");
    let known = typescript.parsed("src/app.ts").expect("parsed before");

    assert!(Rc::ptr_eq(&first, &again) && Rc::ptr_eq(&first, &known));
    assert_eq!(parsed_on_this_thread() - before, 1);
    assert_eq!(source.reads_for_parse.get(), 1);
    assert_eq!(source.reads.get(), 0);
}

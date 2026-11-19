//! The source listing of `owners::Sources` and the `[languages.typescript]` settings that steer it.

use gate_typescript::owners::Sources;
use gate_typescript::{Allocator, MemorySource, Settings, TypeScript};

const FILES: &[(&str, &str)] = &[
    (
        "app/main.ts",
        "import { helper } from \"./util/helper\";\nexport const main = helper;\n",
    ),
    ("app/util/helper.ts", "export const helper = 1;\n"),
    ("app/types.d.ts", "declare const value: number;\n"),
    ("app/view.tsx", "export const view = 1;\n"),
    ("app/Upper.TS", "export {};\n"),
    ("app/broken.ts", "const = ;\n"),
    ("app/dist/built.ts", "export {};\n"),
    ("dist/bundle.ts", "export {};\n"),
    ("app/deep/node_modules/vendored.ts", "export {};\n"),
    ("app/generated/out.ts", "export {};\n"),
    ("other/outside.ts", "export {};\n"),
    ("application.ts", "export {};\n"),
];

fn listed(settings: Settings, root: &str) -> (Vec<String>, Vec<&'static str>) {
    let source = MemorySource::new(FILES.iter().copied());
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, settings, &allocator);
    let sources = Sources::new(&typescript, root).expect("lists");
    let contexts = sources.iter().map(|context| context.path.clone()).collect();
    let unparsed = FILES
        .iter()
        .map(|(path, _)| *path)
        .filter(|path| sources.unparsed(path))
        .collect();
    (contexts, unparsed)
}

#[test]
fn sources_are_the_ts_files_below_the_root_outside_the_skip_folders() {
    let (contexts, unparsed) = listed(Settings::default(), "app");

    assert_eq!(
        contexts,
        [
            "app/dist/built.ts",
            "app/generated/out.ts",
            "app/main.ts",
            "app/util/helper.ts"
        ]
    );
    assert_eq!(unparsed, ["app/broken.ts"]);
}

#[test]
fn an_empty_root_lists_the_whole_source() {
    let (contexts, _) = listed(Settings::default(), "");

    assert_eq!(
        contexts,
        [
            "app/dist/built.ts",
            "app/generated/out.ts",
            "app/main.ts",
            "app/util/helper.ts",
            "application.ts",
            "other/outside.ts",
        ]
    );
}

#[test]
fn a_root_with_a_final_slash_lists_the_same_files() {
    assert_eq!(
        listed(Settings::default(), "app/"),
        listed(Settings::default(), "app")
    );
}

#[test]
fn the_skip_folders_setting_replaces_the_default_list() {
    let settings = Settings {
        skip_folders: vec!["generated".to_owned()],
        ..Settings::default()
    };
    let (contexts, _) = listed(settings, "app");

    assert_eq!(
        contexts,
        [
            "app/deep/node_modules/vendored.ts",
            "app/dist/built.ts",
            "app/main.ts",
            "app/util/helper.ts",
        ]
    );
}

#[test]
fn a_context_resolves_its_imports_to_tracked_files() {
    let source = MemorySource::new(FILES.iter().copied());
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);
    let sources = Sources::new(&typescript, "app").expect("lists");
    let main = sources.get("app/main.ts").expect("listed");

    assert_eq!(
        main.specifier("./util/helper").as_deref(),
        Some("app/util/helper.ts")
    );
}

#[test]
fn settings_read_kebab_case_keys_and_default_each_missing_key() {
    let defaults: Settings = serde_json::from_str("{}").expect("parses");
    let custom: Settings =
        serde_json::from_str(r#"{ "skip-folders": ["out"], "tsconfig": "tsconfig.app.json" }"#)
            .expect("parses");

    assert_eq!(defaults, Settings::default());
    assert_eq!(
        Settings::default().skip_folders,
        [".git", "node_modules", "/build", "/coverage", "/dist"]
    );
    assert_eq!(Settings::default().tsconfig, "tsconfig.json");
    assert_eq!(custom.skip_folders, ["out"]);
    assert_eq!(custom.tsconfig, "tsconfig.app.json");
}

#[test]
fn an_unknown_settings_key_is_an_error() {
    let error =
        serde_json::from_str::<Settings>(r#"{ "skip_folders": [] }"#).expect_err("unknown key");

    assert!(error.to_string().contains("skip_folders"), "{error}");
}

#[test]
fn a_read_source_lists_only_its_paths_and_reads_through_its_function() {
    let read_count = std::cell::Cell::new(0);
    let source = gate_typescript::ReadSource::new(
        vec!["b.ts".to_owned(), "a.ts".to_owned(), "a.ts".to_owned()],
        |path| {
            read_count.set(read_count.get() + 1);
            (path != "b.ts").then(|| b"export const a = 1;\n".to_vec())
        },
    );
    let allocator = Allocator::default();
    let typescript = TypeScript::new(&source, Settings::default(), &allocator);

    assert_eq!(typescript.files(), ["a.ts", "b.ts"]);
    assert_eq!(
        read_count.get(),
        0,
        "nothing is read before it is asked for"
    );
    assert!(typescript.parse("a.ts").expect("reads").is_some());
    assert!(typescript.parse("b.ts").expect("reads").is_none());
    assert!(typescript.parse("c.ts").expect("reads").is_none());
    assert_eq!(read_count.get(), 2, "an untracked path is never read");
}

fn skipping(entries: &[&str]) -> Settings {
    Settings {
        skip_folders: entries.iter().map(|entry| (*entry).to_owned()).collect(),
        ..Settings::default()
    }
}

#[test]
fn the_default_skips_build_at_the_root_and_reads_a_nested_build_folder() {
    let settings = Settings::default();

    assert!(settings.skips("build/main.ts"));
    assert!(settings.skips("coverage/lcov/report.ts"));
    assert!(settings.skips("dist/index.ts"));
    assert!(!settings.skips("src/infrastructure/build/composition.ts"));
    assert!(!settings.skips("test/unit/domain/build/case.ts"));
    assert!(!settings.skips("src/modules/source/coverage/index.ts"));
    assert!(settings.skips("src/deep/node_modules/pkg/index.ts"));
    assert!(settings.skips(".git/hooks/x.ts"));
    assert!(!settings.skips("build.ts"), "a file is not a folder");
}

#[test]
fn an_entry_with_an_inner_slash_names_one_folder_from_the_project_root() {
    let settings = skipping(&["infrastructure/cdk/cdk.out", "/out/"]);

    assert!(settings.skips("infrastructure/cdk/cdk.out/asset.ts"));
    assert!(settings.skips("infrastructure/cdk/cdk.out/deep/asset.ts"));
    assert!(!settings.skips("other/infrastructure/cdk/cdk.out/asset.ts"));
    assert!(!settings.skips("infrastructure/cdk/cdk.output/asset.ts"));
    assert!(!settings.skips("infrastructure/cdk/lib/cdk.out.ts"));
    assert!(
        settings.skips("out/a.ts"),
        "a trailing slash changes nothing"
    );
    assert!(!settings.skips("src/out/a.ts"));
}

#[test]
fn an_entry_without_a_slash_names_a_folder_at_any_depth() {
    let settings = skipping(&["generated"]);

    assert!(settings.skips("generated/a.ts"));
    assert!(settings.skips("src/modules/user/generated/a.ts"));
    assert!(!settings.skips("src/generated.ts"));
    assert!(!settings.skips("src/generatedx/a.ts"));
}

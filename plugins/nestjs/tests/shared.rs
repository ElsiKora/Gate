//! The source walks and path helpers the rules share.

use gate_typescript::{MemorySource, Settings};
use nestjs::layout::Layout;
use nestjs::shared::{modules, tree};

fn source(paths: &[&str]) -> MemorySource {
    MemorySource::new(paths.iter().map(|path| (*path, "")))
}

fn only(folders: &[&str]) -> Settings {
    Settings {
        skip_folders: folders.iter().map(|folder| (*folder).to_owned()).collect(),
        ..Settings::default()
    }
}

#[test]
fn a_walk_skips_a_path_with_a_skipped_folder_at_any_depth_from_the_project_root() {
    let source = source(&[
        "src/a.ts",
        "src/b.d.ts",
        "src/c.js",
        "src/node_modules/x.ts",
        "src/deep/node_modules/y.ts",
        "src/deep/z.ts",
        "srcx/w.ts",
        "node_modules/v.ts",
        "src/.ts",
        "out/app/u.ts",
    ]);
    let settings = only(&["node_modules", "out"]);

    assert_eq!(
        tree::typescript_files(&source, "src", &settings).collect::<Vec<_>>(),
        ["src/a.ts", "src/b.d.ts", "src/deep/z.ts"]
    );
    assert_eq!(
        tree::files(&source, "", &settings).count(),
        6,
        "an empty root walks the whole project"
    );
    assert_eq!(
        tree::files(&source, "out/app", &settings).count(),
        0,
        "a skipped folder above the walk root skips the walk"
    );
    assert!(
        settings.skips("a/out/b.ts") && !settings.skips("a/outer/b.ts") && !settings.skips("out")
    );
}

#[test]
fn folders_and_entries_list_what_holds_a_file_outside_the_skipped_folders() {
    let source = source(&[
        "src/modules/user/user.module.ts",
        "src/modules/user/dto/create.dto.ts",
        "src/modules/user/generated/gen.ts",
        "src/modules/node_modules/pkg/index.ts",
        "src/main.ts",
    ]);
    let settings = only(&["node_modules", "generated"]);

    assert_eq!(
        tree::folders(&source, "src", &settings)
            .into_iter()
            .collect::<Vec<_>>(),
        ["src/modules", "src/modules/user", "src/modules/user/dto"]
    );
    assert_eq!(
        tree::entries(&source, "src/modules/user", &settings)
            .into_iter()
            .collect::<Vec<_>>(),
        [
            ("dto".to_owned(), true),
            ("user.module.ts".to_owned(), false)
        ]
    );
}

#[test]
fn walks_over_nested_roots_read_each_file_once() {
    let source = source(&["tools/lib/a.ts", "src/b.ts"]);
    let settings = Settings::default();

    assert_eq!(
        tree::distinct(
            tree::typescript_files(&source, "", &settings)
                .chain(tree::typescript_files(&source, "tools", &settings))
        ),
        ["src/b.ts", "tools/lib/a.ts"]
    );
}

#[test]
fn test_support_and_scenarios_follow_the_layout() {
    let layout = Layout::default();

    assert!(tree::is_test_support(&layout, "test/fixture/user.ts"));
    assert!(tree::is_test_support(&layout, "test/user.fixture.ts"));
    assert!(!tree::is_test_support(&layout, "test/fixture/user.test.ts"));
    assert!(!tree::is_test_support(&layout, "test/type/user.d.ts"));
    assert!(!tree::is_test_support(&layout, "test/user.helper.ts"));
    assert!(!tree::is_test_support(&layout, "test/fixture/user.case.ts"));
    let custom = Layout {
        test_scenario_suffixes: vec![".spec.ts".to_owned()],
        ..Layout::default()
    };
    assert!(!tree::is_test_support(&custom, "test/fixture/user.spec.ts"));
    assert!(tree::is_test_support(&custom, "test/fixture/user.test.ts"));
    assert!(tree::is_test_scenario(&layout, "test/user.case.ts"));
    assert!(!tree::is_test_scenario(&layout, "test/user.fixture.ts"));
}

#[test]
fn a_relative_specifier_names_a_project_path() {
    assert_eq!(
        modules::relative_target("src/modules/user/user.service.ts", "../shared/x"),
        "src/modules/shared/x"
    );
    assert_eq!(modules::relative_target("src/a.ts", "../../x"), "../x");
    assert_eq!(modules::normalize("src//a/./b/../c/"), "src/a/c/");
    assert_eq!(modules::normalize("../a/.."), "..");
    assert_eq!(modules::normalize("a/.."), ".");
}

#[test]
fn a_path_option_normalizes_its_segments_and_keeps_a_prefix_slash() {
    use nestjs::shared::paths::{Form, check, normalized};

    assert_eq!(
        normalized("./src//a/", Form::Folder).as_deref(),
        Ok("src/a")
    );
    assert_eq!(
        normalized("./src//a/", Form::Prefix).as_deref(),
        Ok("src/a/")
    );
    assert_eq!(
        normalized("src/a/../b.ts", Form::File).as_deref(),
        Ok("src/b.ts")
    );
    assert_eq!(normalized("/src", Form::Folder).as_deref(), Ok("src"));
    assert_eq!(normalized("", Form::Folder).as_deref(), Ok(""));
    assert!(normalized("", Form::File).is_err());
    assert!(normalized("../outside", Form::Folder).is_err());

    assert!(check("root", "src/a/", Form::Prefix).is_ok());
    let error = check("root", "src/a/", Form::Folder)
        .expect_err("a folder has no trailing slash")
        .to_string();
    assert!(
        error.contains("`root`") && error.contains(r#"write "src/a""#),
        "{error}"
    );
}

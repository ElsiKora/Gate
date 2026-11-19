//! `nestjs/kind-layout` against synthetic source trees.

mod host;

use host::{Request, Row, SKIP_GENERATED, check, error, paths_with, row, rows, run_with};

const RULE: &str = "kind-layout";
const EMPTY: &str = "export {};\n";

fn tree(paths: &[&'static str]) -> Vec<(&'static str, &'static str)> {
    paths.iter().map(|path| (*path, EMPTY)).collect()
}

fn sorted(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort();
    rows
}

fn nested(folder: &str) -> Row {
    row(
        "nested-kind-folder",
        folder,
        None,
        &format!("{folder} nests a kind folder inside a role folder"),
    )
}

fn mixed(folder: &str, outer: &str, inner: &str) -> Row {
    row(
        "mixed-kind-folder",
        folder,
        None,
        &format!("{folder} nests a {inner} kind folder inside a {outer} kind folder"),
    )
}

fn utility(folder: &str, root: &str) -> Row {
    row(
        "utility-placement",
        folder,
        None,
        &format!(
            "{folder} is a utility folder outside {root}; helpers there are function/<name>.function.ts files"
        ),
    )
}

fn controller_folder(path: &str) -> Row {
    row(
        "controller-placement",
        path,
        None,
        &format!(
            "{path} is inside a controller kind folder; controller.ts belongs at the owning resource root"
        ),
    )
}

fn controller_name(path: &str) -> Row {
    row(
        "controller-placement",
        path,
        None,
        &format!(
            "{path} uses a prefixed controller filename; use controller.ts at the owning resource root"
        ),
    )
}

fn owner(path: &str, folder: &str) -> Row {
    let name = path.rsplit('/').next().unwrap_or(path);

    row(
        "owner-file-placement",
        path,
        None,
        &format!("{path} is inside a {folder} folder; {name} belongs at the owning resource root"),
    )
}

fn placement(path: &str) -> Row {
    row(
        "subscriber-placement",
        path,
        None,
        &format!("{path} is a subscriber outside a subscriber kind folder"),
    )
}

fn lifecycle(path: &str) -> Row {
    row(
        "subscriber-name",
        path,
        None,
        &format!(
            "{path} names a subscriber by framework layer or lifecycle instead of responsibility"
        ),
    )
}

fn unit(path: &str, modules: &str, target: &str) -> Row {
    row(
        "unit-namespace",
        path,
        None,
        &format!("{path} omits the {modules} namespace; use {target}"),
    )
}

#[test]
fn kind_folders_owner_files_utility_folders_and_subscribers_out_of_place_are_reported() {
    let files = tree(&[
        "src/common/utility/type/value.type.ts",
        "src/common/decorator/type/value.type.ts",
        "src/modules/domain/book/dto/module.ts",
        "src/modules/domain/book/dto/nested/module.ts",
        "src/modules/domain/book/guard/service.ts",
        "src/modules/internal/book/type/controller.ts",
        "src/modules/domain/book/state.subscriber.ts",
        "src/modules/domain/book/subscriber.ts",
        "src/modules/domain/book/subscriber/subscriber.ts",
        "src/modules/domain/book/function/type/value.type.ts",
        "src/modules/domain/book/utility/format.utility.ts",
        "src/shared-kernel/utility/format.utility.ts",
    ]);

    assert_eq!(
        sorted(check(RULE, "{}", &files)),
        sorted(vec![
            mixed("src/common/decorator/type", "decorator", "type"),
            mixed("src/common/utility/type", "utility", "type"),
            utility("src/common/utility", "src/shared"),
            owner("src/modules/domain/book/dto/module.ts", "dto"),
            owner("src/modules/domain/book/dto/nested/module.ts", "dto"),
            owner("src/modules/domain/book/guard/service.ts", "guard"),
            owner("src/modules/internal/book/type/controller.ts", "type"),
            placement("src/modules/domain/book/state.subscriber.ts"),
            placement("src/modules/domain/book/subscriber.ts"),
            placement("src/modules/domain/book/subscriber/subscriber.ts"),
            mixed("src/modules/domain/book/function/type", "function", "type"),
            utility("src/modules/domain/book/utility", "src/shared"),
            utility("src/shared-kernel/utility", "src/shared"),
        ])
    );
}

#[test]
fn role_folders_controllers_lifecycle_subscribers_and_unit_areas_are_reported() {
    let files = tree(&[
        "src/modules/book/guard/dto/request.dto.ts",
        "src/modules/book/strategy/function/check.function.ts",
        "src/shared/subscriber/audit/type/inner/shape.type.ts",
        "src/modules/book/controller/list.ts",
        "src/modules/book/controller/types.d.ts",
        "src/modules/book/controller/admin.controller.ts",
        "src/modules/book/user.controller.ts",
        "src/modules/book/subscriber/route.subscriber.ts",
        "src/modules/book/subscriber/on-before-create.subscriber.ts",
        "src/runtime/subscriber/update.subscriber.ts",
        "src/modules/domain/book.ts",
        "src/modules/infrastructure/book.ts",
        "test/unit/domain/book.test.ts",
        "test/unit/domain/nested/helper.ts",
        "test/unit/infrastructure/other.test.ts",
    ]);

    assert_eq!(
        sorted(check(RULE, "{}", &files)),
        sorted(vec![
            nested("src/modules/book/guard/dto"),
            nested("src/modules/book/strategy/function"),
            nested("src/shared/subscriber/audit/type"),
            nested("src/shared/subscriber/audit/type/inner"),
            controller_folder("src/modules/book/controller/list.ts"),
            controller_folder("src/modules/book/controller/types.d.ts"),
            controller_folder("src/modules/book/controller/admin.controller.ts"),
            controller_name("src/modules/book/controller/admin.controller.ts"),
            controller_name("src/modules/book/user.controller.ts"),
            lifecycle("src/modules/book/subscriber/route.subscriber.ts"),
            lifecycle("src/modules/book/subscriber/on-before-create.subscriber.ts"),
            lifecycle("src/runtime/subscriber/update.subscriber.ts"),
            unit(
                "test/unit/domain/book.test.ts",
                "modules",
                "test/unit/modules/domain/book.test.ts"
            ),
            unit(
                "test/unit/domain/nested/helper.ts",
                "modules",
                "test/unit/modules/domain/nested/helper.ts"
            ),
            unit(
                "test/unit/infrastructure/other.test.ts",
                "modules",
                "test/unit/modules/infrastructure/other.test.ts"
            ),
        ])
    );
}

#[test]
fn files_where_their_kind_belongs_pass() {
    let files = tree(&[
        "src/modules/book/controller.ts",
        "src/modules/book/controller.d.ts",
        "src/modules/book/user.controller.d.ts",
        "src/modules/book/guard/role.guard.ts",
        "src/modules/book/dto/guard/request.dto.ts",
        "src/modules/book/subscriber/state.subscriber.ts",
        "src/modules/book/subscriber/on-load.subscriber.ts",
        "src/modules/book/subscriber/on-before-.subscriber.ts",
        "src/modules/book/subscriber/graph/route.subscriber.ts",
        "src/shared/strategy-helper/type/shape.type.ts",
        "src/shared/utility/format.utility.ts",
        "src/modules/domain/book/module.ts",
        "src/modules/domain/book/service.ts",
        "src/modules/domain/book/policy/service.ts",
        "src/modules/domain/book/dto/registration/dto/request.dto.ts",
        "src/modules/domain/book/function/planner/loan/credit.function.ts",
        "src/modules/domain/book.ts",
        "src/modules/public/book.ts",
        "src/modules/api/book.ts",
        "src/modules/infrastructure/book.ts",
        "src/public/owner.ts",
        "src/node_modules/pkg/utility/type/index.ts",
        "src/modules/node_modules/pkg/controller/index.ts",
        "lib/common/utility/type/value.type.ts",
        "test/unit/modules/domain/book.test.ts",
        "test/unit/api/route.test.ts",
        "test/unit/public/owner.test.ts",
        "test/unit/infrastructure/cdk/stack.test.ts",
        "test/unit/domain/readme.md",
        "test/unit/node_modules/pkg/index.ts",
    ]);

    assert_eq!(check(RULE, "{}", &files), []);
}

#[test]
fn options_change_the_folders_names_and_unit_root() {
    let files = tree(&[
        "src/shared/utility/a.utility.ts",
        "src/common/utility/b.utility.ts",
        "src/modules/book/handler/list.ts",
        "src/modules/book/user.handler.ts",
        "src/modules/book/controller/list.ts",
        "src/modules/book/dto/index.ts",
        "src/modules/book/dto/module.ts",
        "src/modules/book/listener/sync.listener.ts",
        "src/modules/book/listener/after-save.listener.ts",
        "src/modules/book/listener/route.listener.ts",
        "src/modules/book/state.listener.ts",
        "src/modules/book/subscriber/route.subscriber.ts",
        "spec/unit/book/a.test.ts",
        "spec/unit/book/exempt/c.test.ts",
        "test/unit/book/b.test.ts",
    ]);
    let options = r#"{
        "utility-root": "src/common",
        "controller-folder": "handler",
        "owner-files": ["index.ts"],
        "subscriber-folder": "listener",
        "lifecycle-subscriber-names": ["sync"],
        "lifecycle-subscriber-prefixes": ["after-"],
        "unit-root": "spec/unit",
        "unit-categories": [],
        "unit-exempt-prefixes": ["spec/unit/book/exempt/"]
    }"#;

    assert_eq!(
        sorted(check(RULE, options, &files)),
        sorted(vec![
            utility("src/shared/utility", "src/common"),
            controller_folder("src/modules/book/handler/list.ts"),
            controller_name("src/modules/book/user.handler.ts"),
            owner("src/modules/book/dto/index.ts", "dto"),
            lifecycle("src/modules/book/listener/sync.listener.ts"),
            lifecycle("src/modules/book/listener/after-save.listener.ts"),
            placement("src/modules/book/state.listener.ts"),
            unit(
                "spec/unit/book/a.test.ts",
                "modules",
                "spec/unit/modules/book/a.test.ts"
            ),
        ])
    );
}

#[test]
fn the_layout_names_the_kind_role_and_skipped_folders_and_the_modules_root() {
    let files = tree(&[
        "src/features/book/policy/model/a.ts",
        "src/features/book/model/view/b.ts",
        "src/features/book/view/module.ts",
        "src/features/book/guard/dto/d.ts",
        "src/features/book/vendor/controller/e.ts",
        "src/vendor/utility/c.ts",
        "src/modules/book/controller/f.ts",
        "test/unit/book/g.test.ts",
    ]);
    let layout = r#"{ "layout": {
        "modules-root": "src/features",
        "kind-folders": ["model"],
        "exclusive-kind-folders": ["model", "view"],
        "role-folders": ["policy"]
    } }"#;
    let findings = run_with(
        RULE,
        &Request {
            plugin_options: layout,
            languages: r#"{ "typescript": { "skip-folders": ["vendor"] } }"#,
            ..Request::default()
        },
        &files,
    )
    .expect("the rule runs");

    assert_eq!(
        sorted(rows(&findings)),
        sorted(vec![
            nested("src/features/book/policy/model"),
            mixed("src/features/book/model/view", "model", "view"),
            owner("src/features/book/view/module.ts", "view"),
            unit(
                "test/unit/book/g.test.ts",
                "features",
                "test/unit/features/book/g.test.ts"
            ),
        ])
    );
}

#[test]
fn an_unknown_option_is_an_error() {
    assert!(error(RULE, r#"{ "kinds": [] }"#, &[]).contains("kinds"));
}

#[test]
fn a_build_folder_below_the_unit_root_is_read() {
    let files = tree(&[
        "src/modules/domain/a.ts",
        "test/unit/domain/build/service.test.ts",
    ]);

    assert_eq!(
        paths_with(RULE, "{}", "", &files),
        ["test/unit/domain/build/service.test.ts"]
    );
}

#[test]
fn folder_name_options_are_single_names() {
    for option in ["utility-folder", "controller-folder", "subscriber-folder"] {
        for value in ["domain/entity", ""] {
            let message = error(RULE, &format!(r#"{{ "{option}": {value:?} }}"#), &[]);

            assert!(message.contains(option), "{message}");
            assert!(message.contains("single name"), "{message}");
        }
    }
}

#[test]
fn an_empty_utility_root_lets_utility_folders_sit_anywhere() {
    let files = tree(&["src/modules/book/utility/a.ts", "src/shared/utility/b.ts"]);

    assert_eq!(
        paths_with(RULE, r#"{ "utility-root": "src/shared" }"#, "", &files),
        ["src/modules/book/utility"]
    );
    assert_eq!(
        paths_with(RULE, r#"{ "utility-root": "" }"#, "", &files),
        Vec::<String>::new()
    );
}

#[test]
fn a_custom_skip_list_replaces_the_default_for_the_unit_walk() {
    let files = tree(&[
        "src/modules/domain/a.ts",
        "test/unit/domain/generated/a.test.ts",
        "test/unit/domain/node_modules/b.test.ts",
    ]);

    assert_eq!(
        paths_with(RULE, "{}", "", &files),
        ["test/unit/domain/generated/a.test.ts"]
    );
    assert_eq!(
        paths_with(RULE, "{}", SKIP_GENERATED, &files),
        ["test/unit/domain/node_modules/b.test.ts"]
    );
}

#[test]
fn module_areas_are_listed_in_every_folder() {
    // The language list skips the area folder itself; the area still counts.
    let files = tree(&[
        "src/modules/generated/a.ts",
        "test/unit/generated/a.test.ts",
    ]);
    let languages = r#"{ "typescript": { "skip-folders": ["src/modules/generated"] } }"#;

    assert_eq!(
        paths_with(RULE, "{}", languages, &files),
        ["test/unit/generated/a.test.ts"]
    );
}

#[test]
fn an_area_with_its_own_folder_counts_its_skipped_files() {
    // The area's own folder below the source root holds only skipped files; it still exists.
    let files = tree(&[
        "src/modules/book/a.ts",
        "src/book/x.ts",
        "test/unit/book/a.test.ts",
    ]);
    let languages = r#"{ "typescript": { "skip-folders": ["src/book"] } }"#;

    assert_eq!(
        paths_with(RULE, "{}", languages, &files),
        Vec::<String>::new()
    );
}

//! `typescript/barrel-imports` run natively through the SDK against synthetic trees: an import
//! through the barrel and past it, names the barrel exports and does not, owners, exempt test
//! roots, re-exports and their cycles, tsconfig aliases, `import x = require`, nested barrels, a
//! folder without a barrel, and the option errors.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// A fixture tree: tracked paths and their text, no git.
struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
}

impl TreeHost for Fixture {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Fixture {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

fn check(files: &[(&str, &str)], options: &str) -> gate_sdk::Result<Vec<Finding>> {
    check_in(files, options, "{}")
}

/// A run with the `[languages]` settings `languages`, as JSON.
fn check_in(
    files: &[(&str, &str)],
    options: &str,
    languages: &str,
) -> gate_sdk::Result<Vec<Finding>> {
    let fixture = Fixture {
        files: files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
            .collect(),
    };
    let request = CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: "barrel-imports".to_owned(),
            options: options.to_owned(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: "{}".to_owned(),
        languages: languages.to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
    };
    gate_sdk::check(&typescript::PLUGIN, &fixture, &request)
}

/// Each finding as `aspect path:line message`.
fn found(files: &[(&str, &str)], options: &str) -> Vec<String> {
    found_in(files, options, "{}")
}

fn found_in(files: &[(&str, &str)], options: &str, languages: &str) -> Vec<String> {
    check_in(files, options, languages)
        .expect("runs")
        .iter()
        .map(|finding| {
            let Some(Location::File(file)) = &finding.location else {
                panic!("a finding names its file: {finding:?}");
            };
            assert!(finding.help.is_some());
            let line = file.span.as_ref().map_or(0, |span| span.line);
            format!(
                "{} {}:{line} {}",
                finding.aspect, file.path, finding.message
            )
        })
        .collect()
}

/// A folder `src/accounts` with a barrel that exports `UserService` and the type `User`, and files the
/// barrel does not export.
const USERS: &[(&str, &str)] = &[
    (
        "src/accounts/index.ts",
        "export { UserService } from './user.service';\nexport type { User } from './user';\n",
    ),
    (
        "src/accounts/user.service.ts",
        "import { hash } from './hash';\nexport class UserService { id = hash(); }\n",
    ),
    (
        "src/accounts/user.ts",
        "export interface User { id: string }\n",
    ),
    ("src/accounts/hash.ts", "export const hash = () => 'h';\n"),
];

fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    USERS.iter().chain(extra).copied().collect()
}

#[test]
fn imports_through_the_barrel_of_names_it_exports_pass() {
    let files = with(&[(
        "src/app.ts",
        "import { UserService } from './accounts';\nimport type { User } from './accounts/index';\n\
         export const make = (user: User) => new UserService();\n",
    )]);
    assert_eq!(found(&files, "{}"), Vec::<String>::new());
}

#[test]
fn an_import_past_the_barrel_is_a_deep_path_also_when_type_only() {
    let files = with(&[(
        "src/app.ts",
        "import { UserService } from './accounts/user.service';\n\
         import type { User } from './accounts/user';\nimport './accounts/hash';\n",
    )]);
    assert_eq!(
        found(&files, "{}"),
        [
            "deep-path src/app.ts:1 src/app.ts imports ./accounts/user.service (src/accounts/user.service.ts) past the barrel src/accounts/index.ts of src/accounts",
            "deep-path src/app.ts:2 src/app.ts imports ./accounts/user (src/accounts/user.ts) past the barrel src/accounts/index.ts of src/accounts",
            "deep-path src/app.ts:3 src/app.ts imports ./accounts/hash (src/accounts/hash.ts) past the barrel src/accounts/index.ts of src/accounts",
        ]
    );
}

#[test]
fn a_name_the_barrel_does_not_export_is_reported() {
    let files = with(&[(
        "src/app.ts",
        "import { UserService, hash } from './accounts';\nimport type { Hidden } from './accounts';\n\
         export { hash as again } from './accounts';\n",
    )]);
    assert_eq!(
        found(&files, "{}"),
        [
            "not-exported src/app.ts:1 src/app.ts imports hash from the barrel src/accounts/index.ts, which does not export it",
            "not-exported src/app.ts:2 src/app.ts imports Hidden from the barrel src/accounts/index.ts, which does not export it",
            "not-exported src/app.ts:3 src/app.ts imports hash from the barrel src/accounts/index.ts, which does not export it",
        ]
    );
}

#[test]
fn files_inside_the_folder_import_it_directly() {
    let files = with(&[(
        "src/accounts/deep/audit.ts",
        "import { hash } from '../hash';\nimport { UserService } from '../user.service';\n",
    )]);
    assert_eq!(found(&files, "{}"), Vec::<String>::new());
}

/// An owner `src/modules/accounts` with kind children, each with its own barrel.
const OWNER: &[(&str, &str)] = &[
    (
        "src/modules/accounts/index.ts",
        "export * from './services';\n",
    ),
    (
        "src/modules/accounts/services/index.ts",
        "export { UserService } from './user.service';\n",
    ),
    (
        "src/modules/accounts/services/user.service.ts",
        "import { userTable } from '../accounts.constants';\nexport class UserService { table = userTable; }\n",
    ),
    (
        "src/modules/accounts/controllers/index.ts",
        "export { UserController } from './user.controller';\n",
    ),
    (
        "src/modules/accounts/controllers/user.controller.ts",
        "import { UserService } from '../services/user.service';\nexport class UserController { service?: UserService; }\n",
    ),
    (
        "src/modules/accounts/accounts.constants.ts",
        "export const userTable = 'users';\n",
    ),
];

#[test]
fn kind_children_import_each_other_within_their_owner() {
    assert_eq!(
        found(OWNER, r#"{"owners": ["src/modules/*"]}"#),
        Vec::<String>::new()
    );
    // Without the owner the controller is outside the services folder.
    assert_eq!(
        found(OWNER, "{}"),
        [
            "deep-path src/modules/accounts/controllers/user.controller.ts:1 src/modules/accounts/controllers/user.controller.ts imports ../services/user.service (src/modules/accounts/services/user.service.ts) past the barrel src/modules/accounts/services/index.ts of src/modules/accounts/services"
        ]
    );
    // Another owner is outside: it goes through the outermost barrel it is not in.
    let mut files = OWNER.to_vec();
    files.push((
        "src/modules/orders/order.service.ts",
        "import { UserService } from '../accounts/services';\n",
    ));
    assert_eq!(
        found(&files, r#"{"owners": ["src/modules/*"]}"#),
        [
            "deep-path src/modules/orders/order.service.ts:1 src/modules/orders/order.service.ts imports ../accounts/services (src/modules/accounts/services/index.ts) past the barrel src/modules/accounts/index.ts of src/modules/accounts"
        ]
    );
}

#[test]
fn a_test_root_is_outside_unless_exempt() {
    let files = with(&[(
        "test/accounts.spec.ts",
        "import { hash } from '../src/accounts/hash';\n",
    )]);
    let deep = "deep-path test/accounts.spec.ts:1 test/accounts.spec.ts imports ../src/accounts/hash (src/accounts/hash.ts) past the barrel src/accounts/index.ts of src/accounts";
    assert_eq!(found(&files, "{}"), [deep]);
    assert_eq!(
        found(&files, r#"{"exempt": ["test/**"]}"#),
        Vec::<String>::new()
    );
    assert_eq!(found(&files, r#"{"exempt": ["src/**"]}"#), [deep]);
}

#[test]
fn re_exports_through_export_star_and_renames_count_as_exported() {
    let files = [
        (
            "src/shapes/index.ts",
            "export * from './circle';\nexport { inner as Square } from './square';\n\
             export * as geometry from './geometry';\nexport { default } from './default';\n",
        ),
        (
            "src/shapes/circle.ts",
            "export * from './radius';\nexport class Circle {}\nexport default 1;\n",
        ),
        ("src/shapes/radius.ts", "export const radius = 1;\n"),
        ("src/shapes/square.ts", "export class inner {}\n"),
        ("src/shapes/geometry.ts", "export const pi = 3;\n"),
        ("src/shapes/default.ts", "export default class Shape {}\n"),
        (
            "src/app.ts",
            "import Shape, { Circle, radius, Square, geometry } from './shapes';\n\
             import { inner } from './shapes';\nimport * as all from './shapes';\n",
        ),
    ];
    assert_eq!(
        found(&files, "{}"),
        [
            "not-exported src/app.ts:2 src/app.ts imports inner from the barrel src/shapes/index.ts, which does not export it"
        ]
    );
}

#[test]
fn a_cycle_of_export_star_passes_every_name_in_either_order() {
    // Two barrels re-export each other. The answer is the same whichever barrel is read first:
    // `src/c.ts` reads `src/a` first in one tree, `src/b` first in the other.
    let barrels = [
        (
            "src/a/index.ts",
            "export * from '../b';\nexport const x = 1;\n",
        ),
        (
            "src/b/index.ts",
            "export * from '../a';\nexport const y = 1;\n",
        ),
    ];
    for (c, d) in [
        ("import { y } from './a';\n", "import { x } from './b';\n"),
        ("import { x } from './b';\n", "import { y } from './a';\n"),
    ] {
        let files: Vec<_> = barrels
            .iter()
            .copied()
            .chain([("src/c.ts", c), ("src/d.ts", d)])
            .collect();
        assert_eq!(found(&files, "{}"), Vec::<String>::new(), "{c}{d}");
    }
    // A name neither barrel declares is still missing.
    let files: Vec<_> = barrels
        .iter()
        .copied()
        .chain([("src/e.ts", "import { x, z } from './b';\n")])
        .collect();
    assert_eq!(
        found(&files, "{}"),
        [
            "not-exported src/e.ts:1 src/e.ts imports z from the barrel src/b/index.ts, which does not export it"
        ]
    );
}

#[test]
fn an_alias_of_the_tsconfig_paths_resolves_like_a_relative_path() {
    let files = with(&[
        (
            "tsconfig.app.json",
            "{ \"compilerOptions\": { \"paths\": { \"@accounts\": [\"./src/accounts\"], \
             \"@accounts/*\": [\"./src/accounts/*\"] } } }\n",
        ),
        (
            "src/app.ts",
            "import { UserService } from '@accounts';\nimport { hash } from '@accounts/hash';\n",
        ),
    ]);
    assert_eq!(
        found_in(
            &files,
            "{}",
            r#"{"typescript": {"tsconfig": "tsconfig.app.json"}}"#
        ),
        [
            "deep-path src/app.ts:2 src/app.ts imports @accounts/hash (src/accounts/hash.ts) past the barrel src/accounts/index.ts of src/accounts"
        ]
    );
    // Without that tsconfig the aliases resolve to no tracked file, like a package.
    assert_eq!(found(&files, "{}"), Vec::<String>::new());
}

#[test]
fn an_import_equals_require_is_an_import_of_the_whole_module() {
    let files = with(&[(
        "src/app.ts",
        "import accounts = require('./accounts');\nimport hash = require('./accounts/hash');\n\
         import Alias = accounts.UserService;\nexport = accounts;\n",
    )]);
    assert_eq!(
        found(&files, "{}"),
        [
            "deep-path src/app.ts:2 src/app.ts imports ./accounts/hash (src/accounts/hash.ts) past the barrel src/accounts/index.ts of src/accounts"
        ]
    );
}

#[test]
fn a_folder_without_a_barrel_and_a_barrel_whose_exports_are_open_report_no_name() {
    let files = [
        ("src/plain/a.ts", "export const a = 1;\n"),
        (
            "src/open/index.ts",
            "export * from 'some-package';\nexport const b = 1;\n",
        ),
        ("src/open/c.ts", "export const c = 1;\n"),
        (
            "src/app.ts",
            "import { a } from './plain/a';\nimport { anything } from './open';\n",
        ),
    ];
    assert_eq!(found(&files, "{}"), Vec::<String>::new());
}

#[test]
fn nested_barrels_take_the_outermost_one_the_importer_is_outside() {
    let files = [
        ("src/a/index.ts", "export * from './b';\n"),
        ("src/a/b/index.ts", "export const deep = 1;\n"),
        (
            "src/x.ts",
            "import { deep } from './a/b';\nimport { deep as same } from './a';\n",
        ),
        ("src/a/y.ts", "import { deep } from './b';\n"),
    ];
    assert_eq!(
        found(&files, "{}"),
        [
            "deep-path src/x.ts:1 src/x.ts imports ./a/b (src/a/b/index.ts) past the barrel src/a/index.ts of src/a"
        ]
    );
}

#[test]
fn the_barrel_name_is_an_option() {
    let files = [
        ("src/accounts/mod.ts", "export const a = 1;\n"),
        ("src/accounts/b.ts", "export const b = 1;\n"),
        (
            "src/app.ts",
            "import { b } from './accounts/b';\nimport { a } from './accounts/mod';\n",
        ),
    ];
    assert_eq!(found(&files, "{}"), Vec::<String>::new());
    assert_eq!(
        found(&files, r#"{"barrel-name": "mod.ts"}"#),
        [
            "deep-path src/app.ts:1 src/app.ts imports ./accounts/b (src/accounts/b.ts) past the barrel src/accounts/mod.ts of src/accounts"
        ]
    );
}

#[test]
fn the_options_are_checked() {
    for (options, expected) in [
        (
            r#"{"barrel-name": "src/index.ts"}"#,
            "barrel-name \"src/index.ts\" is not a file name",
        ),
        (
            r#"{"barrel-name": ""}"#,
            "barrel-name \"\" is not a file name",
        ),
        (
            r#"{"owners": ["src/[modules"]}"#,
            "owners glob \"src/[modules\" is not valid",
        ),
        (
            r#"{"exempt": ["test/{a"]}"#,
            "exempt glob \"test/{a\" is not valid",
        ),
        (r#"{"barrel": "index.ts"}"#, "unknown field `barrel`"),
    ] {
        let error = check(USERS, options).expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
        assert!(error.is_config(), "{options}: {error}");
    }
    let info = gate_sdk::describe(&typescript::PLUGIN);
    let rule = info
        .rules
        .iter()
        .find(|rule| rule.name == "barrel-imports")
        .expect("the rule");
    assert_eq!(rule.aspects, ["deep-path", "not-exported"]);
    for key in ["barrel-name", "owners", "exempt"] {
        assert!(rule.options_schema.contains(key), "{}", rule.options_schema);
    }
}

//! `nestjs/runtime-factory` against synthetic start files.

mod support;

use support::{check, error, row};

const RULE: &str = "runtime-factory";
const START: &str = "src/runtime/api/function/start.function.ts";

fn options(methods: &str) -> String {
    format!(r#"{{ "contracts": [{{ "file": "{START}", "methods": {methods} }}] }}"#)
}

#[test]
fn a_start_file_that_calls_exactly_its_methods_passes() {
    let source = "import { NestFactory } from \"@nestjs/core\";\n\
                  export async function start(): Promise<void> {\n\
                  \x20 const app = await NestFactory.create(AppModule);\n\
                  \x20 await app.listen(3000);\n\
                  }\n";

    assert_eq!(
        check(RULE, &options(r#"["create"]"#), &[(START, source)]),
        []
    );
}

#[test]
fn a_missing_method_is_reported_once_and_another_method_at_its_first_call() {
    let source = "import { NestFactory } from \"@nestjs/core\";\n\
                  export async function start(): Promise<void> {\n\
                  \x20 await NestFactory.createMicroservice(AppModule);\n\
                  \x20 await NestFactory.createMicroservice(OtherModule);\n\
                  }\n";

    assert_eq!(
        check(
            RULE,
            &options(r#"["create", "create"]"#),
            &[(START, source)]
        ),
        [
            row(
                "",
                START,
                None,
                &format!("{START} must call native NestFactory.create")
            ),
            row(
                "",
                START,
                Some(3),
                &format!(
                    "{START} calls native NestFactory.createMicroservice outside its role contract"
                )
            ),
        ]
    );
}

#[test]
fn aliased_namespace_constant_and_optional_calls_count() {
    let options =
        options(r#"["create", "createApplicationContext", "createMicroservice", "init"]"#);
    let source = "import { NestFactory as Factory } from \"@nestjs/core\";\n\
                  import * as core from \"@nestjs/core\";\n\
                  const Alias = Factory;\n\
                  export async function start(): Promise<void> {\n\
                  \x20 await Factory.create(AppModule);\n\
                  \x20 await core.NestFactory.createApplicationContext(AppModule);\n\
                  \x20 await Alias.createMicroservice(AppModule);\n\
                  \x20 await Factory?.init?.();\n\
                  }\n";

    assert_eq!(check(RULE, &options, &[(START, source)]), []);
}

#[test]
fn a_shadowed_name_is_not_the_factory() {
    let source = "import { NestFactory } from \"@nestjs/core\";\n\
                  export async function start(NestFactory: Local): Promise<void> {\n\
                  \x20 await NestFactory.createMicroservice(AppModule);\n\
                  }\n";

    assert_eq!(
        check(RULE, &options(r#"["create"]"#), &[(START, source)]),
        [row(
            "",
            START,
            None,
            &format!("{START} must call native NestFactory.create")
        )]
    );
}

#[test]
fn a_missing_start_file_is_reported_and_unlisted_files_are_not_read() {
    let other =
        "import { NestFactory } from \"@nestjs/core\";\nNestFactory.createMicroservice(A);\n";

    assert_eq!(
        check(
            RULE,
            &options(r#"["create"]"#),
            &[("src/runtime/worker/start.ts", other)]
        ),
        [row(
            "",
            START,
            None,
            &format!("{START} is required for the native NestFactory role contract")
        )]
    );
}

#[test]
fn the_factory_class_and_package_are_configurable() {
    let source = "import { AppFactory } from \"@scope/core\";\nAppFactory.boot(A);\n";
    let options = format!(
        r#"{{ "factory-class": "AppFactory", "contracts": [{{ "file": "{START}", "methods": ["boot"] }}] }}"#
    );
    let plugin = r#"{ "layout": { "nest-core-package": "@scope/core" } }"#;

    let findings = support::run(RULE, &options, plugin, &[(START, source)]).expect("runs");
    assert_eq!(support::rows(&findings), []);
}

#[test]
fn an_unknown_option_is_an_error() {
    let message = error(
        RULE,
        r#"{ "contracts": [{ "filePath": "a.ts", "methods": [] }] }"#,
        &[],
    );
    assert!(message.contains("filePath"), "{message}");
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

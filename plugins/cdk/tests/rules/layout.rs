//! `cdk/layout`.

use crate::{Files, app, check, findings, messages, seen};

const MONITORING: &str =
    r#"{ "extra": [{ "name": "monitoring", "job": "the alarms and dashboards" }] }"#;

#[test]
fn missing_names_each_required_file_and_directory_and_the_example_configuration() {
    let files = Files::new(&[("cdk.json", "{}\n")]);
    assert_eq!(
        check("layout", &files),
        [
            seen("missing", "bin/application.ts", None, "bin/application.ts"),
            seen("missing", "deploy", None, "deploy"),
            seen("missing", "guard/naming.guard", None, "guard/naming.guard"),
            seen("missing", "lib/config", None, "lib/config"),
            seen("missing", "lib/deployment", None, "lib/deployment"),
            seen("missing", "lib/naming", None, "lib/naming"),
            seen("missing", "lib/stack", None, "lib/stack"),
            seen("missing", "package.json", None, "package.json"),
            seen("missing", "tsconfig.json", None, "tsconfig.json"),
        ]
    );
}

#[test]
fn unknown_names_files_in_lib_and_undeclared_directories_and_says_where_a_reserved_one_goes() {
    let files = app()
        .with("lib/helper.ts", "export {};\n")
        .with("lib/utils/format.utility.ts", "export {};\n")
        .with("lib/monitoring/alarm.construct.ts", "export {};\n");
    assert_eq!(
        check("layout", &files),
        [
            seen("unknown", "lib/helper.ts", None, "lib/helper.ts"),
            seen("unknown", "lib/monitoring", None, "lib/monitoring"),
            seen("unknown", "lib/utils", None, "lib/utils"),
        ]
    );
    let messages = messages("layout", &files);
    assert!(
        messages.contains(
            &"lib/utils is a reserved name; its content goes to the topic that uses it, as a .utility.ts file"
                .to_owned()
        ),
        "{messages:#?}"
    );
}

#[test]
fn a_declared_extension_passes_and_a_declared_one_without_files_is_extra_missing() {
    let files = app().with("lib/monitoring/alarm.construct.ts", "export {};\n");
    assert_eq!(findings("layout", MONITORING, &files), []);
    assert_eq!(
        findings("layout", MONITORING, &app()),
        [seen("extra-missing", "lib", None, "monitoring")]
    );
}

#[test]
fn stack_directory_wants_the_stack_its_barrel_and_its_properties_interface() {
    let files = app()
        .with("lib/stack/storage/storage.stack.ts", "export {};\n")
        .with("lib/stack/shared.stack.ts", "export {};\n");
    assert_eq!(
        check("layout", &files),
        [
            seen(
                "stack-directory",
                "lib/stack/shared.stack.ts",
                None,
                "lib/stack/shared.stack.ts"
            ),
            seen(
                "stack-directory",
                "lib/stack/storage",
                None,
                "lib/stack/storage/index.ts"
            ),
            seen(
                "stack-directory",
                "lib/stack/storage",
                None,
                "lib/stack/storage/interface/index.ts"
            ),
            seen(
                "stack-directory",
                "lib/stack/storage",
                None,
                "lib/stack/storage/interface/properties.interface.ts"
            ),
        ]
    );
}

#[test]
fn entry_names_every_other_file_in_bin() {
    let files = app().with("bin/tools.ts", "export {};\n");
    assert_eq!(
        check("layout", &files),
        [seen("entry", "bin/tools.ts", None, "bin/tools.ts")]
    );
}

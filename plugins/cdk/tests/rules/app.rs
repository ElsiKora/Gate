//! `cdk/app` and `cdk/environment`.

use crate::{CDK_JSON, ENTRY, PACKAGE, STACK, app, check, findings, seen};

const EXAMPLE: &str = "deploy/staging.config.example.json";

/// An account id built at run time, so no twelve-digit literal is tracked.
fn account() -> String {
    "4".repeat(12)
}

#[test]
fn app_command_wants_the_standard_command_in_a_json_object() {
    let files = app().edit(
        CDK_JSON,
        "npx ts-node --prefer-ts-exts bin/application.ts",
        "npx ts-node bin/application.ts",
    );
    assert_eq!(
        check("app", &files),
        [seen("command", CDK_JSON, None, "app")]
    );
    let files = app().with(CDK_JSON, "[]\n");
    assert_eq!(
        check("app", &files),
        [seen("command", CDK_JSON, None, "unreadable")]
    );
}

#[test]
fn app_context_holds_feature_flags_only() {
    let files = app().edit(
        CDK_JSON,
        "\"context\": {\n",
        "\"context\": {\n    \"environment\": \"staging\",\n",
    );
    assert_eq!(
        check("app", &files),
        [seen("context", CDK_JSON, None, "environment")]
    );
}

#[test]
fn app_pin_wants_each_package_at_an_exact_version_or_a_catalog_entry() {
    let files = app().edit(
        PACKAGE,
        "    \"cdk-nag\": \"3.0.0\",\n    \"constructs\": \"10.4.2\"\n",
        "    \"cdk-nag\": \"^3.0.0\"\n",
    );
    assert_eq!(
        check("app", &files),
        [
            seen("pin", PACKAGE, None, "cdk-nag"),
            seen("pin", PACKAGE, None, "constructs"),
        ]
    );
}

#[test]
fn app_scripts_wants_synth_test_and_lint() {
    let files = app().edit(PACKAGE, "    \"lint\": \"eslint .\",\n", "");
    assert_eq!(
        check("app", &files),
        [seen("scripts", PACKAGE, None, "lint")]
    );
}

#[test]
fn environment_tracked_configuration_allows_only_the_example() {
    let files = app().with("deploy/staging.config.json", "{}\n");
    assert_eq!(
        check("environment", &files),
        [seen(
            "tracked-configuration",
            "deploy/staging.config.json",
            None,
            "deploy/staging.config.json"
        )]
    );
}

#[test]
fn environment_account_reports_ids_in_json_and_in_literals() {
    let id = account();
    let files = app().edit(EXAMPLE, "<account>", &id).edit(
        ENTRY,
        "const app = new App();",
        &format!("const app = new App({{ env: {{ account: \"{id}\" }} }});"),
    );
    assert_eq!(
        check("environment", &files),
        [
            seen("account", ENTRY, Some(7), &id),
            seen("account", EXAMPLE, Some(1), &id),
        ]
    );
}

#[test]
fn environment_account_reads_template_literals_and_skips_the_committed_context_file() {
    let id = account();
    let files = app()
        .edit(
            STACK,
            "    const destroy",
            &format!("    const role = `arn:aws:iam::{id}:role/${{id}}`;\n    const destroy"),
        )
        .with(
            "cdk.context.json",
            &format!(
                "{{\n  \"availability-zones:account={id}:region=eu-central-1\": [\"eu-central-1a\"]\n}}\n"
            ),
        );
    assert_eq!(
        check("environment", &files),
        [seen("account", STACK, Some(11), &id)]
    );
}

#[test]
fn environment_region_knows_the_china_and_govcloud_partitions() {
    let files = app().edit(
        STACK,
        "    const destroy",
        "    const china = \"cn-north-1\";\n    const government = \"us-gov-west-1\";\n    const destroy",
    );
    assert_eq!(
        check("environment", &files),
        [
            seen("region", STACK, Some(11), "cn-north-1"),
            seen("region", STACK, Some(12), "us-gov-west-1"),
        ]
    );
    assert_eq!(
        findings(
            "environment",
            r#"{ "regions": ["cn-north-1", "us-gov-west-1"] }"#,
            &files
        ),
        []
    );
}

#[test]
fn environment_region_allows_the_listed_regions_only() {
    let files = app().edit(
        STACK,
        "    const destroy",
        "    const home = \"eu-west-1\";\n    const edge = \"us-east-1\";\n    const destroy",
    );
    assert_eq!(
        check("environment", &files),
        [seen("region", STACK, Some(11), "eu-west-1")]
    );
    assert_eq!(
        findings(
            "environment",
            r#"{ "regions": ["eu-west-1", "us-east-1"] }"#,
            &files
        ),
        []
    );
}

#[test]
fn environment_context_key_reads_only_the_configuration() {
    let files = app().edit(
        ENTRY,
        "tryGetContext(\"configuration\")",
        "tryGetContext(\"stage\")",
    );
    assert_eq!(
        check("environment", &files),
        [seen("context-key", ENTRY, Some(8), "stage")]
    );
}

#[test]
fn environment_process_env_is_read_only_in_lib_config_and_scripts_are_not_judged_for_it() {
    let files = app()
        .edit(
            ENTRY,
            "const app = new App();",
            "const app = new App();\nconst stage = process.env.STAGE;\nprocess.exitCode = 1;",
        )
        .edit(
            "lib/config/read-configuration.function.ts",
            "return value as IConfiguration;",
            "return (process.env.STAGE === undefined ? value : value) as IConfiguration;",
        )
        .with(
            "scripts/synth.ts",
            "const region = \"eu-central-1\";\nexport const stage = process.env.STAGE ?? region;\n",
        );
    assert_eq!(
        check("environment", &files),
        [
            seen("process-env", ENTRY, Some(8), "process.env"),
            seen("region", "scripts/synth.ts", Some(1), "eu-central-1"),
        ]
    );
}

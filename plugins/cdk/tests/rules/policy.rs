//! `cdk/tags`, `cdk/removal-policy`, `cdk/nag` and `cdk/tests`.

use crate::{ENTRY, STACK, TAGGING, app, check, findings, seen};

const POLICY: &str = "removalPolicy: destroy ? RemovalPolicy.DESTROY : RemovalPolicy.RETAIN,";
const REGISTRATION: &str =
    "Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }));";
const OWNER: &str =
    r#"{ "extra-tags": [{ "name": "Owner", "job": "the team that answers for the resource" }] }"#;

#[test]
fn tags_place_reports_a_tag_applied_outside_lib_naming() {
    let files = app()
        .edit(
            STACK,
            "import { RemovalPolicy, Stack } from",
            "import { RemovalPolicy, Stack, Tags } from",
        )
        .edit(
            STACK,
            "    applyTags(",
            "    Tags.of(this).add(\"Project\", \"sample\");\n    this.tags.setTag(\"Name\", \"network\");\n    applyTags(",
        );
    assert_eq!(
        check("tags", &files),
        [
            seen("place", STACK, Some(17), "Project"),
            seen("place", STACK, Some(18), "Name"),
        ]
    );
}

#[test]
fn tags_key_allows_the_standard_and_the_declared_keys_and_required_wants_each_standard_key() {
    let files = app().edit(TAGGING, "add(\"ManagedBy\"", "add(\"Owner\"");
    assert_eq!(
        check("tags", &files),
        [
            seen("key", TAGGING, Some(7), "Owner"),
            seen("required", "lib/naming", None, "ManagedBy"),
        ]
    );
    assert_eq!(
        findings("tags", OWNER, &files),
        [seen("required", "lib/naming", None, "ManagedBy")]
    );
}

#[test]
fn removal_policy_destroy_passes_on_a_branch_whose_other_branch_keeps_the_data() {
    let files = app().edit(STACK, POLICY, "removalPolicy: RemovalPolicy.RETAIN,").edit(
        STACK,
        "    applyTags(",
        "    if (destroy) {\n      this.node.addMetadata(\"policy\", RemovalPolicy.DESTROY);\n    }\n    applyTags(",
    );
    assert_eq!(check("removal-policy", &files), []);
}

#[test]
fn removal_policy_destroy_reports_an_unconditional_destroy_and_a_choice_of_two() {
    let files = app().edit(STACK, POLICY, "removalPolicy: RemovalPolicy.DESTROY,");
    assert_eq!(
        check("removal-policy", &files),
        [seen("destroy", STACK, Some(15), "DESTROY")]
    );
    let files = app().edit(
        STACK,
        POLICY,
        "removalPolicy: destroy ? RemovalPolicy.DESTROY : RemovalPolicy.DESTROY,",
    );
    assert_eq!(
        check("removal-policy", &files),
        [
            seen("destroy", STACK, Some(15), "DESTROY"),
            seen("destroy", STACK, Some(15), "DESTROY"),
        ]
    );
}

#[test]
fn removal_policy_auto_delete_reports_a_literal_true() {
    let files = app().edit(
        STACK,
        "autoDeleteObjects: destroy,",
        "autoDeleteObjects: true,",
    );
    assert_eq!(
        check("removal-policy", &files),
        [seen("auto-delete", STACK, Some(13), "autoDeleteObjects")]
    );
}

#[test]
fn nag_missing_and_pack_judge_the_registered_packs() {
    let files = app().edit(ENTRY, REGISTRATION, "");
    assert_eq!(check("nag", &files), [seen("missing", ENTRY, None, "nag")]);
    assert_eq!(
        findings(
            "nag",
            r#"{ "nag-packs": ["AwsSolutionsChecks", "ServerlessChecks"] }"#,
            &app()
        ),
        [seen("pack", ENTRY, None, "ServerlessChecks")]
    );
}

#[test]
fn nag_form_reports_a_pack_added_as_an_aspect_and_nag_suppressions() {
    let files = app()
        .edit(
            ENTRY,
            "import { App, Validations } from",
            "import { App, Aspects } from",
        )
        .edit(
            ENTRY,
            REGISTRATION,
            "Aspects.of(app).add(new AwsSolutionsChecks(app, { verbose: true }));",
        )
        .edit(
            STACK,
            "import type { Construct } from \"constructs\";",
            "import type { Construct } from \"constructs\";\nimport { NagSuppressions } from \"cdk-nag\";",
        )
        .edit(
            STACK,
            "    applyTags(",
            "    NagSuppressions.addResourceSuppressions(this, []);\n    applyTags(",
        );
    assert_eq!(
        check("nag", &files),
        [
            seen(
                "form",
                ENTRY,
                Some(10),
                "Aspects.of(…).add(new AwsSolutionsChecks(…))"
            ),
            seen("form", STACK, Some(18), "NagSuppressions"),
            seen("missing", ENTRY, None, "nag"),
        ]
    );
}

#[test]
fn tests_wants_a_unit_test_for_each_stack_and_each_naming_and_configuration_function() {
    let files = app()
        .without("test/stack/network/network.stack.test.ts")
        .without("test/naming/apply-tags.function.test.ts");
    assert_eq!(
        check("tests", &files),
        [
            seen(
                "module",
                TAGGING,
                None,
                "test/naming/apply-tags.function.test.ts"
            ),
            seen(
                "stack",
                STACK,
                None,
                "test/stack/network/network.stack.test.ts"
            ),
        ]
    );
}

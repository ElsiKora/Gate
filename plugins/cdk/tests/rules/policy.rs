//! `cdk/tags`, `cdk/removal-policy`, `cdk/nag` and `cdk/tests`.

use crate::{ENTRY, PACKAGE, STACK, TAGGING, app, check, findings, messages, seen};

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

/// The tag function, applying the tags of a map in a loop; `MAP` is where the map comes from.
const TAG_LOOP: &str = r#"import { Tags } from "aws-cdk-lib";
import type { IConstruct } from "constructs";

const STANDARD = { Application: "sample", ManagedBy: "cdk", Project: "sample" };

export function applyTags(scope: IConstruct, environment: string, name: string): void {
  const tags = { ...STANDARD, Environment: environment, Name: name, Team: "platform" };
  for (const [key, value] of Object.entries(MAP)) {
    Tags.of(scope).add(key, value);
  }
}
"#;

#[test]
fn tags_reads_the_keys_of_a_same_file_map_applied_in_a_loop() {
    let files = app().with(TAGGING, &TAG_LOOP.replace("MAP", "tags"));
    assert_eq!(
        check("tags", &files),
        [seen("key", TAGGING, Some(9), "Team")]
    );
    // A map that cannot be read gives no finding, and no standard key is reported missing.
    let files = app().with(TAGGING, &TAG_LOOP.replace("MAP", "readTags(tags)"));
    assert_eq!(check("tags", &files), []);
}

#[test]
fn removal_policy_destroy_passes_on_a_branch_whose_other_branch_keeps_the_data() {
    let files = app().edit(STACK, POLICY, "removalPolicy: RemovalPolicy.RETAIN,").edit(
        STACK,
        "    applyTags(",
        "    if (destroy) {\n      this.node.addMetadata(\"policy\", RemovalPolicy.DESTROY);\n    } else {\n      this.node.addMetadata(\"policy\", RemovalPolicy.SNAPSHOT);\n    }\n    applyTags(",
    );
    assert_eq!(check("removal-policy", &files), []);
}

#[test]
fn removal_policy_destroy_reports_a_default_and_an_if_with_no_keeping_branch() {
    let files = app()
        .edit(
            STACK,
            POLICY,
            "removalPolicy: properties.configuration.policy ?? RemovalPolicy.DESTROY,",
        )
        .edit(
            STACK,
            "    applyTags(",
            "    if (destroy) {\n      this.node.addMetadata(\"policy\", RemovalPolicy.DESTROY);\n    }\n    applyTags(",
        );
    assert_eq!(
        check("removal-policy", &files),
        [
            seen("destroy", STACK, Some(15), "DESTROY"),
            seen("destroy", STACK, Some(18), "DESTROY"),
        ]
    );
}

#[test]
fn removal_policy_judges_only_the_removal_policy_of_the_library() {
    let files = app()
        .edit(
            STACK,
            "import { RemovalPolicy, Stack } from",
            "import { Stack } from",
        )
        .edit(
            STACK,
            "export class NetworkStack",
            "const RemovalPolicy = { DESTROY: \"destroy\", RETAIN: \"retain\" };\n\nexport class NetworkStack",
        )
        .edit(STACK, POLICY, "removalPolicy: RemovalPolicy.DESTROY,");
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
fn removal_policy_auto_delete_follows_its_sibling_destroy() {
    // Beside a DESTROY chosen against RETAIN, a literal true is allowed.
    let files = app().edit(
        STACK,
        "autoDeleteObjects: destroy,",
        "autoDeleteObjects: true,",
    );
    assert_eq!(check("removal-policy", &files), []);
    // Beside an unconditional DESTROY, both are reported.
    let files = app()
        .edit(
            STACK,
            "autoDeleteObjects: destroy,",
            "autoDeleteObjects: true,",
        )
        .edit(STACK, POLICY, "removalPolicy: RemovalPolicy.DESTROY,");
    assert_eq!(
        check("removal-policy", &files),
        [
            seen("auto-delete", STACK, Some(13), "autoDeleteObjects"),
            seen("destroy", STACK, Some(15), "DESTROY"),
        ]
    );
    // Beside no removal policy, or one that keeps the data, it is reported.
    for policy in ["", "removalPolicy: RemovalPolicy.RETAIN,"] {
        let files = app()
            .edit(
                STACK,
                "autoDeleteObjects: destroy,",
                "autoDeleteObjects: true,",
            )
            .edit(STACK, POLICY, policy);
        assert_eq!(
            check("removal-policy", &files),
            [seen("auto-delete", STACK, Some(13), "autoDeleteObjects")],
            "{policy}"
        );
    }
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
fn nag_version_reports_cdk_nag_before_3_and_names_the_v3_form() {
    let files = app().edit(
        PACKAGE,
        "\"cdk-nag\": \"3.0.0\"",
        "\"cdk-nag\": \"2.37.55\"",
    );
    assert_eq!(
        check("nag", &files),
        [seen("version", PACKAGE, None, "cdk-nag")]
    );
    let said = messages("nag", &files);
    assert!(
        said.iter().any(|message| message
            .contains("Validations.of(app).addPlugins(new AwsSolutionsChecks(app))")),
        "{said:?}"
    );
}

#[test]
fn nag_reads_a_pack_held_in_a_constant() {
    let files = app().edit(
        ENTRY,
        REGISTRATION,
        "const checks = new AwsSolutionsChecks(app, { verbose: true });\nValidations.of(app).addPlugins(checks);",
    );
    assert_eq!(check("nag", &files), []);
}

#[test]
fn nag_target_reports_a_pack_registered_on_a_stage_and_validations_must_be_the_library_one() {
    let files = app()
        .edit(
            ENTRY,
            "new ApplicationDeployment(app,",
            "const deployment = new ApplicationDeployment(app,",
        )
        .edit(ENTRY, "Validations.of(app)", "Validations.of(deployment)");
    assert_eq!(
        check("nag", &files),
        [
            seen("missing", ENTRY, None, "nag"),
            seen("target", ENTRY, Some(10), "AwsSolutionsChecks"),
        ]
    );
    let files = app()
        .edit(
            ENTRY,
            "import { App, Validations } from \"aws-cdk-lib\";",
            "import { App } from \"aws-cdk-lib\";\nconst Validations = { of: (_scope: unknown) => ({ addPlugins: (..._packs: unknown[]) => undefined }) };",
        );
    assert_eq!(check("nag", &files), [seen("missing", ENTRY, None, "nag")]);
}

#[test]
fn nag_acknowledgment_wants_a_reason() {
    let files = app().edit(
        ENTRY,
        REGISTRATION,
        &format!(
            "{REGISTRATION}\nValidations.of(app).acknowledge({{ id: \"AwsSolutions-S1\" }});\nValidations.of(app).acknowledge({{ id: \"AwsSolutions-IAM4\", reason: \" \" }});\nValidations.of(app).acknowledge({{ id: \"AwsSolutions-L1\", reason: \"the platform pins the runtime\" }});"
        ),
    );
    assert_eq!(
        check("nag", &files),
        [
            seen("acknowledgment", ENTRY, Some(11), "AwsSolutions-S1"),
            seen("acknowledgment", ENTRY, Some(12), "AwsSolutions-IAM4"),
        ]
    );
}

#[test]
fn a_file_that_does_not_parse_gives_no_finding() {
    let files = app().with(ENTRY, "const = ;\n");
    assert_eq!(check("nag", &files), []);
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

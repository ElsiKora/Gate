//! `[plugins.cdk] options`: each refused value fails the run with a configuration error that names
//! the key and the value.

use crate::{app, error};

/// The error without the rule the SDK names first.
fn refused(options: &str) -> String {
    let message = error("layout", options, &app());
    message
        .strip_prefix("rule layout: ")
        .unwrap_or_else(|| panic!("the error names the rule: {message}"))
        .to_owned()
}

#[test]
fn extra_refuses_a_name_out_of_case_a_standard_name_a_reserved_name_and_an_empty_job() {
    assert_eq!(
        refused(r#"{ "extra": [{ "name": "Monitoring", "job": "alarms" }] }"#),
        "[plugins.cdk] options: extra names `Monitoring`, which is not kebab case"
    );
    assert_eq!(
        refused(r#"{ "extra": [{ "name": "stack", "job": "stacks" }] }"#),
        "[plugins.cdk] options: extra names `stack`, which is a directory of the standard already"
    );
    assert_eq!(
        refused(r#"{ "extra": [{ "name": "utils", "job": "helpers" }] }"#),
        "[plugins.cdk] options: extra names `utils`, which is reserved: its content goes to the topic that uses it, as a .utility.ts file"
    );
    assert_eq!(
        refused(r#"{ "extra": [{ "name": "monitoring", "job": " " }] }"#),
        "[plugins.cdk] options: extra names `monitoring`, which has no job; say in one sentence what it holds"
    );
}

#[test]
fn extra_tags_refuses_a_key_out_of_case_and_a_standard_key() {
    assert_eq!(
        refused(r#"{ "extra-tags": [{ "name": "owner", "job": "the team" }] }"#),
        "[plugins.cdk] options: extra-tags names `owner`, which is not Pascal case"
    );
    assert_eq!(
        refused(r#"{ "extra-tags": [{ "name": "Project", "job": "the project" }] }"#),
        "[plugins.cdk] options: extra-tags names `Project`, which is a tag of the standard already"
    );
}

#[test]
fn regions_and_nag_packs_refuse_what_is_not_one() {
    assert_eq!(
        refused(r#"{ "regions": ["moon-base-1"] }"#),
        "[plugins.cdk] options: regions names `moon-base-1`, which is not an AWS region"
    );
    assert_eq!(
        refused(r#"{ "nag-packs": [] }"#),
        "[plugins.cdk] options: nag-packs is empty; name at least one cdk-nag pack, such as AwsSolutionsChecks"
    );
    assert_eq!(
        refused(r#"{ "nag-packs": ["StrictChecks"] }"#),
        "[plugins.cdk] options: nag-packs names `StrictChecks`, which is not a cdk-nag pack; use one of AwsSolutionsChecks, HIPAASecurityChecks, NIST80053R4Checks, NIST80053R5Checks, PCIDSS321Checks, ServerlessChecks"
    );
}

#[test]
fn an_unknown_option_is_refused() {
    let message = refused(r#"{ "prefix": "sample" }"#);
    assert!(message.contains("prefix"), "{message}");
}

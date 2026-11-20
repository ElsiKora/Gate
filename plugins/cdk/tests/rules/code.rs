//! `cdk/stack-names`, `cdk/construct-ids`, `cdk/physical-names` and `cdk/properties`.

use crate::{DEPLOYMENT, STACK, app, check, seen};

const STACK_ID: &str =
    "new NetworkStack(this, stackName, { configuration: properties.configuration, stackName });";
const BUCKET_NAME: &str =
    "bucketName: createPhysicalName(properties.configuration.environment, \"flow-logs\"),";
const PARAMETER: &str = "properties: INetworkStackProperties) {";

#[test]
fn stack_names_topic_wants_the_directory_and_the_file_to_name_one_topic() {
    let files = app().moved(STACK, "lib/stack/network/vpc.stack.ts");
    assert_eq!(
        check("stack-names", &files),
        [seen("topic", "lib/stack/network/vpc.stack.ts", None, "vpc")]
    );
}

#[test]
fn stack_names_id_wants_a_naming_call_with_the_topic() {
    let files = app().edit(
        DEPLOYMENT,
        "environment, \"network\")",
        "environment, \"storage\")",
    );
    assert_eq!(
        check("stack-names", &files),
        [seen("id", DEPLOYMENT, Some(12), "NetworkStack")]
    );
    let files = app().edit(
        DEPLOYMENT,
        STACK_ID,
        "new NetworkStack(this, \"Network\", { configuration: properties.configuration });",
    );
    assert_eq!(
        check("stack-names", &files),
        [seen("id", DEPLOYMENT, Some(12), "NetworkStack")]
    );
}

#[test]
fn stack_names_stack_name_wants_the_id() {
    let files = app().edit(
        DEPLOYMENT,
        STACK_ID,
        "new NetworkStack(this, stackName, { configuration: properties.configuration, stackName: \"Network\" });",
    );
    assert_eq!(
        check("stack-names", &files),
        [seen("stack-name", DEPLOYMENT, Some(12), "NetworkStack")]
    );
}

#[test]
fn construct_ids_case_wants_pascal_case_literals_and_templates() {
    let files = app()
        .edit(STACK, "this, \"FlowLogs\"", "this, \"flowLogs\"")
        .edit(
            STACK,
            "    applyTags(",
            "    new Bucket(this, `Archive${destroy}`);\n    new Bucket(this, `archive-${destroy}`);\n    applyTags(",
        );
    assert_eq!(
        check("construct-ids", &files),
        [
            seen("case", STACK, Some(12), "flowLogs"),
            seen("case", STACK, Some(18), "archive-${…}"),
        ]
    );
}

#[test]
fn construct_ids_duplicate_reports_a_second_id_in_one_scope() {
    let files = app().edit(
        STACK,
        "    applyTags(",
        "    new Bucket(this, \"FlowLogs\");\n    new Bucket(scope, \"FlowLogs\");\n    applyTags(",
    );
    assert_eq!(
        check("construct-ids", &files),
        [seen("duplicate", STACK, Some(17), "FlowLogs")]
    );
}

#[test]
fn construct_ids_judges_constructions_only_and_the_literal_parts_of_a_template() {
    let files = app().edit(
        STACK,
        "    applyTags(",
        "    const pattern = new RegExp(id, \"u\");\n    const start = new Date(destroy, \"utc\");\n    new Bucket(this, `${id}Archive`);\n    new Bucket(this, `${id}-archive`);\n    applyTags(",
    );
    assert_eq!(
        check("construct-ids", &files),
        [seen("case", STACK, Some(20), "${…}-archive")]
    );
}

#[test]
fn construct_ids_duplicate_allows_one_id_in_branches_that_exclude_each_other() {
    let files = app().edit(
        STACK,
        "    applyTags(",
        "    if (destroy) {\n      new Bucket(this, \"Archive\");\n    } else {\n      new Bucket(this, \"Archive\");\n    }\n    const log = destroy ? new Bucket(this, \"Log\") : new Bucket(this, \"Log\");\n    applyTags(",
    );
    assert_eq!(check("construct-ids", &files), []);
}

#[test]
fn physical_names_literal_reports_concatenations_constants_and_props_built_in_a_constant() {
    for value in ["bucketName: \"sample-\" + destroy,", "bucketName: name,"] {
        let files = app()
            .edit(
                STACK,
                "    const destroy",
                "    const name = \"sample-flow-logs\";\n    const destroy",
            )
            .edit(STACK, BUCKET_NAME, value);
        assert_eq!(
            check("physical-names", &files),
            [seen("literal", STACK, Some(15), "bucketName")],
            "{value}"
        );
    }
    let files = app()
        .edit(
            STACK,
            "    new Bucket(this, \"FlowLogs\", {",
            "    const props = {",
        )
        .edit(
            STACK,
            "    });\n    applyTags(",
            "    };\n    new Bucket(this, \"FlowLogs\", props);\n    applyTags(",
        )
        .edit(STACK, BUCKET_NAME, "bucketName: `sample-${destroy}`,");
    assert_eq!(
        check("physical-names", &files),
        [seen("literal", STACK, Some(14), "bucketName")]
    );
}

#[test]
fn physical_names_literal_reports_strings_templates_and_calls_outside_lib_naming() {
    for value in [
        "bucketName: \"sample-flow-logs\",",
        "bucketName: `sample-${destroy}`,",
        "bucketName: String(destroy),",
    ] {
        let files = app().edit(STACK, BUCKET_NAME, value);
        assert_eq!(
            check("physical-names", &files),
            [seen("literal", STACK, Some(14), "bucketName")],
            "{value}"
        );
    }
}

#[test]
fn properties_parameter_and_interface_judge_the_third_constructor_parameter() {
    let files = app().edit(STACK, PARAMETER, "props: INetworkStackProperties) {");
    assert_eq!(
        check("properties", &files),
        [seen("parameter", STACK, Some(9), "NetworkStack")]
    );
    let files = app().edit(STACK, PARAMETER, "properties: StackProps) {");
    assert_eq!(
        check("properties", &files),
        [seen("interface", STACK, Some(9), "NetworkStack")]
    );
    let files = app().edit(
        STACK,
        "from \"./interface/properties.interface\"",
        "from \"../../config\"",
    );
    assert_eq!(
        check("properties", &files),
        [seen("interface", STACK, Some(9), "NetworkStack")]
    );
}

#[test]
fn properties_judges_constructs_and_names_props_interfaces() {
    let interface = "lib/stack/network/interface/properties.interface.ts";
    let construct = "lib/construct/archive/archive.construct.ts";
    let files = app()
        .edit(interface, "INetworkStackProperties", "INetworkStackProps")
        .with(
            construct,
            "import { Construct } from \"constructs\";\n\nexport class ArchiveConstruct extends Construct {\n  constructor(scope: Construct, id: string, props: IArchiveProperties) {\n    super(scope, id);\n  }\n}\n",
        );
    assert_eq!(
        check("properties", &files),
        [
            seen("interface", construct, Some(4), "ArchiveConstruct"),
            seen("interface", interface, Some(5), "INetworkStackProps"),
            seen("parameter", construct, Some(4), "ArchiveConstruct"),
        ]
    );
}

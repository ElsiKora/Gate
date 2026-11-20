//! `cdk/container-health`.

use crate::{app, check, seen};

const SERVICE: &str = "lib/stack/service/service.stack.ts";

/// A stack that defines containers every way the rule reads: in place, through a `const`, through
/// a helper that builds the props, through a helper the props pass through, and with props that
/// cannot be read. `§` marks where each readable definition gives its health check.
const SERVICE_TEXT: &str = r#"import { Stack } from "aws-cdk-lib";
import { ContainerDefinition, ContainerImage, FargateTaskDefinition } from "aws-cdk-lib/aws-ecs";
import type { Construct } from "constructs";

const healthCheck = { command: ["CMD-SHELL", "true"] };
const image = ContainerImage.fromRegistry("sample");
const shared = { §image };

function webProps(name: string) {
  return { ...shared, containerName: name };
}

function addWorker(task: FargateTaskDefinition, props: { image: ContainerImage }): void {
  task.addContainer("Worker", props);
}

export class ServiceStack extends Stack {
  constructor(scope: Construct, id: string) {
    super(scope, id);
    const task = new FargateTaskDefinition(this, "Task");
    task.addContainer("Web", { §image });
    task.addContainer("Api", webProps("api"));
    new ContainerDefinition(this, "Sidecar", { §image, taskDefinition: task });
    addWorker(task, { §image });
    task.addContainer("Loaded", load());
    task.addContainer("Spread", { ...load(), image });
  }
}
"#;

fn service(health: bool) -> String {
    SERVICE_TEXT.replace('§', if health { "healthCheck, " } else { "" })
}

#[test]
fn container_health_passes_each_definition_that_gives_a_health_check() {
    let files = app().with(SERVICE, &service(true));
    assert_eq!(check("container-health", &files), []);
}

#[test]
fn container_health_missing_reports_in_place_helper_and_passed_through_props() {
    let files = app().with(SERVICE, &service(false));
    assert_eq!(
        check("container-health", &files),
        [
            seen("missing", SERVICE, Some(21), "Web"),
            seen("missing", SERVICE, Some(22), "Api"),
            seen("missing", SERVICE, Some(23), "Sidecar"),
            seen("missing", SERVICE, Some(24), "Worker"),
        ]
    );
}

#[test]
fn container_health_reads_the_ecs_namespace_of_the_library() {
    let files = app().with(
        SERVICE,
        "import { Stack, aws_ecs as ecs } from \"aws-cdk-lib\";\n\nexport class ServiceStack extends Stack {\n  build(task: ecs.TaskDefinition, image: ecs.ContainerImage): void {\n    new ecs.ContainerDefinition(this, \"Web\", { image, taskDefinition: task });\n  }\n}\n",
    );
    assert_eq!(
        check("container-health", &files),
        [seen("missing", SERVICE, Some(5), "Web")]
    );
}

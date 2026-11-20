# `cdk/container-health`

Every ECS container definition has a health check.

## What it checks and why

ECS replaces a task whose container stops answering only when the container has a health check;
without one, a hung process keeps its place behind the load balancer until someone notices. The
rule judges the container definitions of `aws-cdk-lib/aws-ecs` in `bin/` and `lib/`:
`taskDefinition.addContainer(id, props)` and `new ContainerDefinition(scope, id, props)` (also as
`aws_ecs.ContainerDefinition` of `aws-cdk-lib`).

- `missing`: the props of a container definition have no `healthCheck`.

The props are read where they are written: an object literal in place or bound to a `const` of the
same file, with the object literals it spreads; the one value a same-file function returns, when the
props are a call of it; and, when the props are a parameter of a same-file function that passes them
through, the argument of each call of that function in the file, reported at that call. Props that
cannot be read this way (a call of an imported function, a spread of one) give no finding.

The `taskImageOptions` of the `aws-ecs-patterns` constructs are not judged: their type,
`ApplicationLoadBalancedTaskImageOptions` in aws-cdk-lib 2.277.0, takes no container health check.
A pattern's container gets one through its `taskDefinition`, which this rule judges.

Scope: project. Reads history: no. Aspects: `missing`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/container-health#missing lib/stack/service/service.stack.ts:21
  lib/stack/service/service.stack.ts defines the container Web without a healthCheck
```

## How to fix it

Give the container a `healthCheck` with the command that proves it answers, and its interval,
timeout and retries:

```ts
task.addContainer("Web", {
  image,
  healthCheck: { command: ["CMD-SHELL", "curl -f http://localhost/health || exit 1"] },
});
```

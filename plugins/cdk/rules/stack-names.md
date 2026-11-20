# `cdk/stack-names`

A stack's directory, file, class and id name one topic, and the id is built by a `lib/naming/`
function.

## What it checks and why

A stack's id is its CloudFormation name: built in one place with the project and the environment,
it never collides with another environment's stack, and the topic finds the stack's code.

- `topic`: `lib/stack/<directory>/<name>.stack.ts` where the name is not the directory.
- `id`: `new <Topic>Stack(scope, id, …)`, the class imported from `lib/stack/`, whose id is not a call
  of a function imported from `lib/naming/` with the topic in kebab case as a string argument,
  directly or through a variable it initializes.
- `stack-name`: the same construction sets `stackName` to another value than the id.

Scope: project. Reads history: no. Aspects: `topic`, `id`, `stack-name`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/stack-names#id lib/deployment/application.deployment.ts:12
  lib/deployment/application.deployment.ts gives NetworkStack an id that is not a call of a lib/naming/ function with the topic "network"
```

## How to fix it

Name the directory, the file and the class with the same topic; build the id with the naming function
of stack names, passing the topic; pass the id as `stackName`, or leave `stackName` out.

# `cdk/physical-names`

Resource names and export names come from a `lib/naming/` function, never written in place.

## What it checks and why

A name AWS gives a resource is global to an account or a region: built in one place with the project
and the environment, two environments never claim the same name, and a guard rule can check the
form. The rule judges the props of every construct instantiation of `bin/` and `lib/` (as
`cdk/construct-ids` defines one): an object literal in place or bound to a `const` of the same file,
with the object literals it spreads.

- `literal`: a property that names a resource (`bucketName`, `exportName`, `functionName`,
  `tableName`, `roleName`, `queueName`, `topicName` and the other `…Name` properties of the
  standard's list) is a string built in place: a string literal, a template literal, a concatenation
  with one, or a call of a function not imported from `lib/naming/`, written in place or bound to a
  `const` of the same file. Any other value, such as a parameter, passes.

Scope: project. Reads history: no. Aspects: `literal`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/physical-names#literal lib/stack/network/network.stack.ts:14
  lib/stack/network/network.stack.ts sets bucketName in place, not through a lib/naming/ function
```

## How to fix it

Build the name with the naming function that gives every name its project and environment, or leave
the property out and let CloudFormation name the resource.

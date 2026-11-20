# `cdk/removal-policy`

Data is destroyed only on a branch of a choice whose other branch keeps it.

## What it checks and why

`RemovalPolicy.DESTROY` and `autoDeleteObjects: true` delete data when a stack is removed or a
resource replaced. A short-lived environment may want that; production must not get it by accident,
so the choice comes from the configuration and names both outcomes.

A choice is a conditional that selects between `DESTROY` and a policy that keeps the data
(`RETAIN`, `RETAIN_ON_UPDATE_OR_DELETE` or `SNAPSHOT`): the other branch of a conditional
expression, the other branch of an `if` with an `else`, or another case of a `switch`.

- `destroy`: `RemovalPolicy.DESTROY`, resolved through `aws-cdk-lib`, that is not one branch of a
  choice within its function. A default (`policy ?? RemovalPolicy.DESTROY`) is not a choice, nor is
  an `if` with no branch that keeps the data. A local object named `RemovalPolicy` is not judged.
- `auto-delete`: a property `autoDeleteObjects: true` written as a literal, unless the same object's
  `removalPolicy` (in place or through a `const` of the same file) holds a `DESTROY` and every
  `DESTROY` it holds is one branch of a choice: it is allowed exactly when that `DESTROY` is.

Scope: project. Reads history: no. Aspects: `destroy`, `auto-delete`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/removal-policy#destroy lib/stack/network/network.stack.ts:15
  lib/stack/network/network.stack.ts sets RemovalPolicy.DESTROY without a choice that keeps the data
```

## How to fix it

Read the choice from the configuration and make `DESTROY` one branch of it, the other `RETAIN`,
`RETAIN_ON_UPDATE_OR_DELETE` or `SNAPSHOT`: `destroy ? RemovalPolicy.DESTROY : RemovalPolicy.RETAIN`.
Set `autoDeleteObjects` from the same value, or `true` beside that `removalPolicy`.

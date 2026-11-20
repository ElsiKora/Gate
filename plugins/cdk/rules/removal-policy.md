# `cdk/removal-policy`

Data is destroyed only on a branch of a choice whose other branch keeps it.

## What it checks and why

`RemovalPolicy.DESTROY` and `autoDeleteObjects: true` delete data when a stack is removed or a
resource replaced. A short-lived environment may want that; production must not get it by accident,
so the choice comes from the configuration and names both outcomes.

- `destroy`: `RemovalPolicy.DESTROY`, resolved through `aws-cdk-lib`, outside a branch of a choice
  within its function: one branch of a conditional whose other branch does not destroy too, or an
  `if`, a `switch` case or a logical expression.
- `auto-delete`: a property `autoDeleteObjects: true` written as a literal.

Scope: project. Reads history: no. Aspects: `destroy`, `auto-delete`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/removal-policy#destroy lib/stack/network/network.stack.ts:15
  lib/stack/network/network.stack.ts sets RemovalPolicy.DESTROY unconditionally
```

## How to fix it

Read the choice from the configuration and make `DESTROY` one branch of it, the other `RETAIN`,
`RETAIN_ON_UPDATE_OR_DELETE` or `SNAPSHOT`: `destroy ? RemovalPolicy.DESTROY : RemovalPolicy.RETAIN`.
Set `autoDeleteObjects` from the same value.

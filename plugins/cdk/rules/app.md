# `cdk/app`

`cdk.json` runs the standard command with feature flags only, and `package.json` pins the CDK
packages and has the standard scripts.

## What it checks and why

The same command runs every app, a value that differs between environments lives in the
configuration file and not in the CDK context, and an exact pin keeps a synthesis reproducible.

- `command`: `cdk.json` is not a JSON object, or its `app` is not
  `npx ts-node --prefer-ts-exts bin/application.ts`.
- `context`: a key of `cdk.json`'s `context` is not a feature flag (a feature flag has a `:`, such as
  `@aws-cdk/core:enableStackNameDuplicates`).
- `pin`: `package.json` is not a JSON object, or one of `aws-cdk-lib`, `aws-cdk`, `constructs` and
  `cdk-nag` is missing from `dependencies` and `devDependencies` or is not an exact version (`2.180.0`;
  a workspace catalog entry, `catalog:`, counts as exact).
- `scripts`: `package.json` has no `synth`, `test` or `lint` script.

Scope: project. Reads history: no. Aspects: `command`, `context`, `pin`, `scripts`.

## Options

None.

## Example finding

```text
error cdk/app#pin package.json
  package.json depends on cdk-nag as `^3.0.0`, which is not an exact version
```

## How to fix it

Set `app` to the standard command, move project values from the context into the configuration
file, pin each package to an exact version or a catalog entry, and add the missing script.

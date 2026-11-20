# `cdk/tests`

Each stack and each naming and configuration function has a unit test at its mirror path.

## What it checks and why

A stack's test holds its synthesized template to what the app means to deploy, and the naming and
configuration functions decide every name and every environment value; a change to either is caught
before a deploy.

- `stack`: `lib/stack/<topic>/<name>.stack.ts` has no `test/stack/<topic>/<name>.stack.test.ts`.
- `module`: a `function` or `utility` file of `lib/naming/` or `lib/config/` has no test at
  `test/<path below lib>.test.ts`.

Scope: project. Reads history: no. Aspects: `stack`, `module`.

## Options

None.

## Example finding

```text
error cdk/tests#stack lib/stack/network/network.stack.ts
  lib/stack/network/network.stack.ts has no unit test at test/stack/network/network.stack.test.ts
```

## How to fix it

Add the unit test at the mirror path of the file below `test/`.

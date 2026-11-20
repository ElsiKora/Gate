# `cdk/construct-ids`

Construct ids written in place are Pascal case, and one scope gives no two constructs the same id.

## What it checks and why

A construct id is part of each resource's logical id: one style keeps the template readable, and two
constructs with one id in one scope fail the synthesis late. The rule judges every
`new X(scope, id, …)` of `bin/` and `lib/` whose scope is `this` or a name and whose id is a string or
template literal.

- `case`: the id is not Pascal case; a template counts when its first part starts with a capital
  and every part is letters and digits.
- `duplicate`: a second construction in the same function gives the same scope the same string id.

Scope: project. Reads history: no. Aspects: `case`, `duplicate`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/construct-ids#case lib/stack/network/network.stack.ts:12
  lib/stack/network/network.stack.ts gives a construct the id "flowLogs", which is not Pascal case
```

## How to fix it

Write the id in Pascal case and give each construct of a scope its own id. A changed id replaces the
resource it names, so rename the id of a stateful resource with care.

# `cdk/construct-ids`

Construct ids written in place are Pascal case, and one scope gives no two constructs the same id.

## What it checks and why

A construct id is part of each resource's logical id: one style keeps the template readable, and two
constructs with one id in one scope fail the synthesis late. The rule judges every construct
instantiation of `bin/` and `lib/`: `new X(scope, id, …)` whose scope is `this` or a name, whose id
is a string or template literal, and whose `X` is a class of `aws-cdk-lib` or `constructs`, or a
class of the app, in the file or imported, that extends one. `new RegExp(pattern, "u")` is not one.

- `case`: the id is not Pascal case. Of a template only the literal parts are judged: each is
  letters and digits, and the first starts with a capital unless the template starts with a
  substitution (`${prefix}Archive` passes).
- `duplicate`: a second construction in the same function gives the same scope the same string id,
  unless the two lie in branches that exclude each other: the two branches of an `if` with an
  `else` or of a conditional, or two cases of a `switch`.

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

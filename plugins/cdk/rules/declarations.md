# `cdk/declarations`

A code file declares one top-level entity; below `lib/` it is of its role's kind, named from the file.

## What it checks and why

One entity per file keeps names, files and imports one to one, so a rename moves one file and a
barrel re-exports each entity once. The entities are the top-level classes, interfaces, type aliases
and enums, exported or not, the exported functions (overloads count once) and constants (one whose
value is an arrow or function expression counts as a function), and the default export. The rule
reads the code of the whole app: `bin/`, `lib/`, `test/` and `scripts/`. Barrels (`index.ts`) are
not judged, nor files of `lib/` whose name has no role; `kind` and `name` judge only files of
`lib/`, the folder where a file's name gives its role.

- `count`: the file declares more than one entity.
- `kind`: an entity is not of the kind its role wants: a class in a `stack`, `construct`,
  `deployment`, `aspect`, `synthesizer` or `error` file, an interface in an `interface` file, a type
  alias in a `type` file, an enum in an `enum` file, a function in a `function` file, a constant in a
  `constant` file. A class, interface, type alias or enum in a file of another role is reported too.
- `name`: the class of a class role is not `Pascal(<name>) + Pascal(<role>)`: `network.stack.ts`
  declares `NetworkStack`.
- `nested`: a class, interface, type alias or enum is declared below the top level.

Scope: project. Reads history: no. Aspects: `count`, `name`, `kind`, `nested`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/declarations#name lib/deployment/application.deployment.ts:8
  lib/deployment/application.deployment.ts declares the class Deployment; its file names it ApplicationDeployment
```

## How to fix it

Move each entity into a file of its own, named for it, with the role of its kind; name a class from
its file; move a nested declaration to the top level of a file of its own.

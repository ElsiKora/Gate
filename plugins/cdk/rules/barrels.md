# `cdk/barrels`

Each folder below `lib/` that holds TypeScript files has an `index.ts` that only re-exports each of
its files and child folders.

## What it checks and why

Code imports a folder through its barrel, so a file can move inside the folder without touching its
importers, and the barrel is the one list of what the folder offers. The rule stays on `lib/`: a
barrel serves importers, and only `lib/` is imported. `bin/application.ts` is the entry the CDK runs,
the tests are run by the test runner and the scripts by hand; nothing imports them.

- `missing`: a folder holds `.ts` files but no `index.ts`.
- `reexport-only`: the barrel holds a directive or a statement other than `export * from`,
  `export { … } from` or `export { … }`.
- `explicit-export`: a file of the folder, or a child folder with a barrel, is not re-exported.
  `./name.ts`, `./name.js` and `./child/index` count as `./name` and `./child`.
- `outside`: the barrel re-exports a specifier that does not start with `./` or that climbs with
  `..`.

Scope: project. Reads history: no. Aspects: `missing`, `reexport-only`, `explicit-export`,
`outside`. Language: TypeScript.

## Options

None.

## Example finding

```text
error cdk/barrels#explicit-export lib/naming/index.ts
  lib/naming/index.ts does not re-export ./create-stack-name.function
```

## How to fix it

Add an `index.ts` with one `export * from "./<file>";` per file and child folder, move any code out of
it into a file of its own, and re-export only what is inside the folder.

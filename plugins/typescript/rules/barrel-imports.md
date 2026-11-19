# `typescript/barrel-imports`

A folder with a barrel is imported from outside only through its barrel, and only for names it
exports.

## What it checks and why

A barrel (`index.ts` by default) is a folder's public surface: what it exports is what the rest of
the code may use, and everything else in the folder can change without touching its importers. An
import that reaches past the barrel into a file of the folder, or that asks the barrel for a name it
does not export, ties the importer to the folder's insides.

A folder has a barrel when it holds a tracked file named `barrel-name`. For each TypeScript file
(`.ts`, `.tsx`, `.mts`, `.cts`, declarations included), the rule reads its static imports and
re-exports:

- `import … from "x"`, `import type … from "x"`, `import "x"`, `import name = require("x")`;
- `export { … } from "x"`, `export type { … } from "x"`, `export * from "x"`,
  `export * as name from "x"`.

It resolves each specifier the way the plugin resolves modules: relative paths and the `paths` of
the tsconfig `[languages.typescript] tsconfig` names. A specifier that does not resolve to a tracked
file (a package) is not checked. Then it takes the outermost folder that holds the imported file and
a barrel, but not the importer:

- **`#deep-path`**: the imported file is not that folder's barrel. With nested barrels, `src/a` and
  `src/a/b`, a file outside `src/a` imports `src/a/b` through `src/a/index.ts`; a file inside
  `src/a` imports it through `src/a/b/index.ts`.
- **`#not-exported`**: the imported file is the barrel, and a name the import takes is not one the
  barrel exports. The barrel's exports are its exported declarations, `export { a, b as c }`,
  `export default`, `export { a as b } from`, `export * as name from`, and every name but the default
  of the modules `export * from` names, followed through tracked files. A default import takes the
  name `default`; `import * as`, `import name = require` and `export *` take no single name and are
  not checked. A cycle of `export *` passes every name of each barrel on it. When the barrel's
  exports cannot all be known (an `export =`, or `export * from` a module that does not resolve to
  a tracked file), no name is reported.

Type-only imports follow the same rule. Files inside a folder import its files directly, and a
folder without a barrel is not checked. The rule does not read a `require(…)` call or a dynamic
`import()`.

**Owners.** Inside an owner, a folder `owners` matches, the kind children of the owner (its
subfolders, each with its own barrel) import each other and the owner's files directly: a folder
inside an owner that holds the importer is skipped. With `owners = ["src/modules/*"]`,
`src/modules/accounts/controllers/user.controller.ts` may import
`src/modules/accounts/services/user.service.ts`; `src/modules/orders/order.service.ts` imports it
through `src/modules/accounts/index.ts`.

**Test roots.** A test file outside the folder is an importer like any other. An importer that
`exempt` matches is not checked: `exempt = ["test/**"]` lets the tests under `test/` import any file
directly. A test file inside the folder imports it directly anyway.

Globs are matched against paths relative to the project root; `*` does not cross `/`. Folders
`[languages.typescript] skip-folders` names are not read as importers. A file that does not parse is
skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `deep-path`, `not-exported`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `barrel-name` | string | `"index.ts"` | The file name of a folder's barrel; a name, not a path. |
| `owners` | array of glob | `[]` | The owner folders whose kind children import each other directly. |
| `exempt` | array of glob | `[]` | The importing files the rule does not check, such as test roots. |

```toml
[rules]
"typescript/barrel-imports" = { level = "error", options = { owners = ["src/modules/*"], exempt = ["test/**", "**/*.e2e-spec.ts"] } }
```

## Example finding

`src/app.ts`, where `src/accounts/index.ts` is the barrel of `src/accounts`:

```ts
import { UserService } from './accounts/user.service';
```

```text
error typescript/barrel-imports#deep-path src/app.ts:1
  src/app.ts imports ./accounts/user.service (src/accounts/user.service.ts) past the barrel
  src/accounts/index.ts of src/accounts
  help: import from the folder's barrel src/accounts/index.ts instead; a name the barrel does not
  export is internal to src/accounts: export it from the barrel, or keep the import inside the folder
```

```ts
import { hash } from './accounts';
```

```text
error typescript/barrel-imports#not-exported src/app.ts:1
  src/app.ts imports hash from the barrel src/accounts/index.ts, which does not export it
  help: export the name from the barrel src/accounts/index.ts, or keep the import inside src/accounts
```

The finding's target is the importing file's path.

## How to fix it

Import the name from the folder's barrel: `import { UserService } from './accounts';`. When the barrel
does not export the name, the name is internal to the folder: either it belongs to the folder's
public surface, and the barrel exports it, or the code that needs it moves into the folder. A folder
whose subfolders are meant to import each other directly is an owner: name it in `owners`.

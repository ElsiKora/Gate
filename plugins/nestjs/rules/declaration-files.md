# `nestjs/declaration-files`

Files own one exported entity, and named contracts sit in their own kind-folder files.

## What it checks and why

A file named for one owner is where a reader looks for that owner. A second class, interface, type or
enum in it hides from that search; a contract declared inside a function cannot be imported; and a
contract outside its kind folder is missed by the barrel and the layout rules that read kind folders.

The files read are the `.ts` files below `layout.source-root` and `layout.cdk-root`, and the test-support
and scenario files below `layout.test-root`, each once, outside `[languages.typescript] skip-folders`. A
barrel (`*/index.ts`) is skipped unless it is a scenario. Declaration files are included.

A file's entities are its exported declarations and exported function-valued or class-valued constants
(a function's overloads count once), its default export, and every class, interface, type alias and
enum in it. Entities are named with the compiler's node names (`ClassDeclaration`,
`FunctionValuedConst`, …); an unnamed one is `<anonymous>`, a default export expression `<default>`.

- `#single-owner`: a file outside `layout.test-root` has more than one entity. Reported at the file, with
  the list of entities.
- `#support-owner`: the same for a test-support file, where all function declarations and
  function-valued constants count as one.
- `#nested-declaration`: a class, interface, type alias or enum below the top level of its file. A class
  expression is top level when it is exported by default, initializes a top-level variable, or is
  assigned to an identifier in a top-level statement. Reported at the declaration.
- `#kind-folder-declaration`: a contract whose kind `contract-kinds` names is not in the last folder of
  that name on its path, lies below another `layout.artifact-folders` folder, or sits in a file not named
  `*.<folder>.ts` or `<folder>.ts`. Reported at the declaration.
- `#test-scenario-declaration`: a test scenario (`layout.test-scenario-suffixes`) declares a class,
  interface, type alias or enum. An unnamed class expression used inline, not bound to a name or
  exported, is allowed. Reported at the declaration.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `kind-folder-declaration`,
`nested-declaration`, `single-owner`, `support-owner`, `test-scenario-declaration`. Findings carry no
target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `contract-kinds` | map of node name to folder | `{ EnumDeclaration = "enum", InterfaceDeclaration = "interface", TypeAliasDeclaration = "type" }` | The folder each kind of contract belongs in. |

```toml
[rules]
"nestjs/declaration-files" = { level = "error" }
```

## Example finding

```text
error nestjs/declaration-files#kind-folder-declaration src/modules/user/user.service.ts:2
  src/modules/user/user.service.ts declares Row; interface contracts belong in the owning interface/ folder with a .interface.ts filename
```

## How to fix it

Move each class, interface, type and enum to its own file; put a contract in a file named for its kind
inside its kind folder, at the top level.

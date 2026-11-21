# `nestjs/barrel-exports`

Source folders expose re-export-only barrels without private entrypoints.

## What it checks and why

A folder's barrel is its public surface: an importer names the folder, not a file inside it. A folder
without a barrel, or a barrel that misses a file, makes importers reach past it; a barrel that holds code
hides an owner in a file named for something else; a wildcard re-export of anything but a child folder
exposes more than the folder owns; and a barrel that re-exports a runtime's start file pulls a whole
application into whoever imports the folder.

A file is an exportable entry of a folder below it when it is a `.ts` file other than a barrel, is not
in a `hidden-folder` below that folder, and is not listed in `private-entrypoints`.

- `#runtime-entrypoint`: a `.ts` file below `layout.source-root` whose path ends with
  `entrypoint-suffix` and that `private-entrypoints` does not approve. Unset, `private-entrypoints`
  approves the one entry point NestJS starts from: the source root's file named by
  `entrypoint-suffix`, `<layout.source-root>/main.ts` by default (`src/main.ts`). Reported at the
  file.
- `#missing`: a folder below `layout.source-root` with an exportable entry has no `barrel-name` file;
  and a folder named in `layout.kind-folders` that holds a `.ts` file other than a barrel has none.
  Reported at the barrel's path.
- `#explicit-export`: the barrel of such a folder does not re-export, by a relative specifier without
  its script extension, each `.ts` file of the folder and each child folder with an exportable entry,
  other than `optional-child-folders` and private entrypoints. A kind folder's barrel must re-export
  each `.ts` file with a double-quoted specifier. A re-export of no names counts as none. Reported at
  the barrel.
- `#reexport-only`: a barrel below `layout.source-root`, or below `layout.test-root` in a test-support
  path, holds a directive or a statement other than an export declaration.
  Reported at the first one.
- `#private-entrypoint`: such a barrel, outside `layout.test-root`, re-exports a module that resolves
  to a listed private entrypoint. Reported at the re-export.
- `#wildcard`: such a barrel has an `export * from` without a name whose specifier is not a child
  folder of the barrel's folder that holds files and that the specifier resolves into. Reported at the
  re-export.

A finding that two of these checks both make is reported once. Folders and files below
`[languages.typescript] skip-folders` are skipped, in every walk: the barrels, their entries and the
child folders a wildcard names.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `explicit-export`, `missing`,
`private-entrypoint`, `reexport-only`, `runtime-entrypoint`, `wildcard`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `private-entrypoints` | list of paths | unset: the source root's file named by `entrypoint-suffix`, `["<layout.source-root>/main.ts"]` | The runtime start files: never exported, never an entry of their folder. A list replaces the default, so list the source root's `main.ts` too; `[]` approves none. |
| `barrel-name` | string | `"index.ts"` | The barrel's file name. |
| `optional-child-folders` | list of names | `["function", "internal", "type", "utility"]` | Child folders a barrel may leave out. |
| `hidden-folder` | string | `"internal"` | The folder whose files are no entry of an enclosing folder. |
| `entrypoint-suffix` | string | `"/main.ts"` | The ending of a start file's path. |
| `script-extensions` | list of strings | `[".cjs", ".cts", ".js", ".mjs", ".mts", ".ts"]` | The extensions a relative re-export may name its target with. |

```toml
[rules]
"nestjs/barrel-exports" = { level = "error", options = { private-entrypoints = ["src/main.ts", "src/runtime/api/main.ts"] } }
```

## Example finding

```text
error nestjs/barrel-exports#explicit-export src/modules/user/dto/index.ts
  src/modules/user/dto/index.ts must export ./update.dto
```

## How to fix it

Give the folder a barrel that re-exports each of its files and child folders by name, move any code out
of the barrel into its own file, and list each runtime start file in the rule's option
`private-entrypoints`, in its `[rules]` entry.

# `nestjs/kind-imports`

Every source and test file loads each declaration from the one module the layout gives it: the
barrel of the kind folder that declares it, or the declaring file itself.

## What it checks and why

A kind folder holds one kind of file (`function/`, `dto/`, `enum/`) behind its own barrel. When every
file loads a declaration from the same module, a mock replaces what the code under test really
loads, a deferred `import()` really defers, and the specifiers show the dependency graph. A load
through a file of a kind folder, a semantic subfolder or a wider barrel hides which kind folder a
file depends on; a load through a barrel of a file that sits in no kind folder hides which file it
needs.

Scope: project. Language: `typescript`. Reads history: no. Findings are located at the load's line,
carry no target, and are keyed by the specifier as written.

### The tree it reads

- **Files read:** every `.ts` file below `layout.source-root` and below `layout.test-root`,
  declaration files included and folders named in `[languages.typescript] skip-folders` left out.
  A file without a syntax tree is skipped; `typescript/parse` reports it.
- **Loads read:** `import … from`, `import "…"`, `export … from`, `export * from`, `export * as … from`
  (type-only forms included), `import x = require("…")`, `import("…")` types, dynamic `import("…")`,
  `require("…")` with one string argument and, below the test root, the first string argument of
  each call in `mock-calls`.
- **Resolution:** a relative specifier resolves from the importer's folder, an alias through
  `layout.path-aliases`, and a path through the source root from the project root; the script
  extension is dropped and `<p>.ts`, `<p>.d.ts`, `<p>/index.ts` and `<p>/index.d.ts` are tried.
  Only a load that resolves to a tracked file below the source root is checked. The tsconfig `paths`
  are not read.
- **Structural positions:** the source root, each `<source-root>/<owner>` of `layout.source-owners`
  and each `<modules-root>/<plane>` of `layout.module-planes`.
- **Kind folder:** a folder below the source root, named by one of `kinds`, that is no structural
  position, that holds at least one `.ts` file other than a `barrel-name` file at any depth, whose
  every such file is named `<name>.<kind>.ts` or `<kind>.ts` (declaration files count, as
  `<name>.<kind>.d.ts`), and that lies below no other kind folder. The folders below it are its
  semantic subfolders; a file is in the kind folder that holds it at any depth.
- **Resource of a folder:** the folder that holds its kind folder, when it is or lies below one;
  otherwise the nearest of the folder and its ancestors below the source root that has a kind folder
  somewhere below it (never `layout.modules-root` or a module plane); otherwise the module folder
  (`<modules-root>/<plane>/<name>`) that holds it, or else its top-level owner.
- **Barrels:** a kind barrel is `<kind folder>/<barrel-name>`; an internal barrel is
  `<kind folder>/<internal-folder>/<barrel-name>`. The module plane named like `internal-folder` is a
  structural position, not such a child.
- **Private file:** a file other than a barrel that no barrel's `export … from` resolves to.
- **Names a load takes:** the names in its braces (`a as b` takes `a`), the destructured property
  names of `const { a, b: c } = await import("…")`, `default` for a default import, `T` of
  `import("…").T`, and every name the module exports for `import * as`, `export *`, `export * as`,
  `import x = require`, `require`, `typeof import("…")` and an `import()` that is not destructured.
  Each name is followed to its declaring file through `export { … } from`, `export * from` and
  `export { a }` of an imported binding. A name with no declaring file (a default export, a name
  behind `export =`) is not checked.

### The module each name is loaded from

For an importer, a name it takes from module `M`, the name's declaring file `D` and the kind folder
`K` that holds `D`, tried in order:

| Case | Condition | Module | Wording |
| --- | --- | --- | --- |
| no kind folder | `D` is in no kind folder | `D` | `by its file D` |
| own | the importer is below the source root and in `K` | `D` | `by its file D` |
| internal | `D` lies below `K/<internal-folder>/`, which has a barrel | the internal barrel | `through the internal barrel K/internal` |
| private | `K` has no barrel, or `D` is private | `D` | `by its file D` |
| kind barrel | the kind barrel exports the name, declared in `D` | the kind barrel | `through the kind barrel K` |
| left out | otherwise | the kind barrel | `through the kind barrel K once it exports them` |

A load is in the form for a name when that module is `M` and the case is not "left out".

### `kind-barrel`

A load, other than a side-effect import, a mock call or a direct-child re-export, that takes a name
out of the form whose case is not "own": a declaration of a kind folder loaded through its file, a
semantic subfolder or a wider barrel; an internal child loaded other than through its own barrel; a
file in no kind folder or a private file loaded through a barrel; a barrel that re-exports past a
kind barrel. Names are grouped by module and sorted, joined by `, `; groups are ordered by module and
joined by ` and `.

A direct-child re-export is an `export … from "./<name>"` of a barrel, one segment: the chain of
barrels. `kind-barrel`, `own-kind-folder` and `resource-relative` never report it.

### `own-kind-folder`

- An importer in a kind folder loads a name declared in that kind folder through a barrel of it or a
  wider barrel, instead of by its file.
- An importer in a kind folder loads a file of that kind folder, already in the form, by a
  non-relative specifier, when a relative path with at most one `../` reaches the file.

### `resource-relative`

A load of a file below the source root by a relative specifier with at most one `..` segment, other
than a direct-child re-export, that resolves into the importer's top-level owner but into another
resource than the importer's. These are the relative loads that `nestjs/import-aliases#owner-alias`
passes.

### `index-segment`

A checked load, mock calls and direct-child re-exports included, whose specifier's last segment is
`index`, with or without a script extension.

### `dynamic-import`

- An `import()`, below either root, whose names are loaded, under the table above, from more than
  one module. A barrel is shown by its folder, a file by its path.
- Below the source root, an `import()` whose module, or the declaring file of a name it takes, the
  file already loads statically: through its value loads (imports and `export … from` that are not
  type-only, side-effect imports, `import x = require` and `require`), transitively.

### `mock-target`

- A call of `mock-calls` whose first argument resolves to a module below the source root that no
  source file loads under the form. A module counts as loaded under the form when it is a kind
  barrel, an internal barrel, a non-barrel file in no kind folder, a private file, or a file of a
  kind folder that another non-barrel file of the same kind folder loads. A resource barrel never
  counts.
- Two or more calls of `hoisted-mock-calls` in one file that resolve to one module: the test runner
  hoists such calls and keeps only the last factory. Reported once per module, at the first call.

### Left to other rules

- **The migration registry.** The load of `registry-import` by `registry` is skipped by every aspect;
  `nestjs/migrations` owns it. Keep the two options equal to that rule's.
- **An entity's enum files.** A file directly in an `entity-folder` folder that loads an enum file
  (`*.<enum-kind>`, any script extension) by a relative, source-root or alias path is skipped by every
  aspect; `nestjs/import-aliases#entity-enum-barrel` reports it, whether the enum file is the
  entity's own resource's or another's. The skip is exact only when `entity-folder` and `enum-kind`
  equal the options of `nestjs/import-aliases`. An entity's load of another resource's enum barrel is
  in the form for both rules.
- **Barrel shape.** Which files a barrel re-exports, and which wildcards it may use, is
  `nestjs/barrel-exports`. That rule decides a kind folder by its name in `layout.kind-folders`; this
  rule decides it by `kinds` and the names of the files it holds.
- **Deep traversal and relative loads across an owner** are `nestjs/import-aliases#owner-alias`.

## With `typescript/barrel-imports`

`typescript/barrel-imports` asks for another form: a load of a file from outside the outermost folder
with a barrel that holds the file goes through that barrel. A resource barrel above a kind folder is
such a folder, so for an importer outside the resource it asks for the resource barrel where this
rule asks for the kind barrel, and for a file in no kind folder it asks for a barrel where this rule
asks for the file.

TO BE WRITTEN after the comparison on the synthetic trees.

## Options

Path options are relative to the project root and in normal form; any other form is an options
error that shows the normal form. An unknown key, or a value of the wrong type, is an options error
that names the key.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `kinds` | list of names | `layout.exclusive-kind-folders` | The kinds whose folders are kind folders. Not empty, no name twice, each one folder name. |
| `barrel-name` | string | `"index.ts"` | The barrel's file name. |
| `internal-folder` | string | `"internal"` | The child of a kind folder whose files its own barrel exports. |
| `registry` | string | `"src/migration/typeorm/migrations.ts"` | The registry of `nestjs/migrations`. |
| `registry-import` | string | `"./generated"` | The registry's specifier of the migrations, as `nestjs/migrations` sets it. |
| `mock-calls` | list of dotted names | `["vi.mock", "vi.doMock", "vi.importActual", "vi.importMock"]` | The calls whose first argument names a module, below the test root. |
| `hoisted-mock-calls` | list of dotted names | `["vi.mock"]` | The calls of `mock-calls` the test runner hoists and keeps once per module. Each one listed in `mock-calls`, no name twice. |
| `entity-folder` | string | `"entity"` | The folder whose files are entities, as `nestjs/import-aliases` sets it. |
| `enum-kind` | string | `"enum"` | The kind of an enum file, as `nestjs/import-aliases` sets it. |

The rule also reads `layout.source-root`, `test-root`, `source-owners`, `module-planes`,
`modules-root`, `path-aliases` and `exclusive-kind-folders`. No preset turns it on.

```toml
[rules]
"nestjs/kind-imports" = { level = "error", options = { kinds = ["constant", "dto", "entity", "enum", "function", "interface"] } }
```

## Example findings

```text
error nestjs/kind-imports#kind-barrel src/modules/author/service.ts:1
  src/modules/author/service.ts loads @modules/book; import bookFormat through the kind barrel src/modules/book/function
error nestjs/kind-imports#own-kind-folder src/modules/book/function/is/available.function.ts:1
  src/modules/book/function/is/available.function.ts loads .. from inside its kind folder src/modules/book/function; import bookFormat by its file src/modules/book/function/format.function.ts
error nestjs/kind-imports#mock-target test/modules/author/service.test.ts:3
  test/modules/author/service.test.ts names @modules/book in vi.doMock, which no source file loads under the form; name the kind barrel or file that the source loads
```

## How to fix it

Load each name from the module the message names: the kind barrel, the internal barrel, or the file.
Inside a kind folder, load its files by their paths. Load another resource through its owner alias.
Drop a trailing `/index`. Split an `import()` per module, and move a deferred module behind a barrel
the file does not load statically. Mock the module the source loads, with one factory per module.

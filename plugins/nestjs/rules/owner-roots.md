# `nestjs/owner-roots`

Every accepted owner root holds source, the modules root holds only the module planes, and no retired
owner remains.

## What it checks and why

A service's architecture names its owners: the module planes, the infrastructure owner and so on. An
owner that exists only on paper, a module folder outside the accepted planes, or a composition
owner that was retired but left behind all make the tree disagree with the architecture that the
other rules enforce.

Scope: project. Language: `typescript`. Reads history: no. Findings are located at the reported path
and carry no target.

### `owner-root`

- A `required-roots` entry with no tracked `.ts` file below it. Only the `.ts` files below
  `layout.source-root` count, declaration files included, in every folder (the walk skips none of
  `[languages.typescript] skip-folders`), so a root outside the source root is never satisfied. The
  folders inside `layout.modules-root` are listed in every folder too.
- A folder directly inside `layout.modules-root` that holds a tracked file (of any extension) and is
  not one of `layout.module-planes`, reported at the folder.

### `shared-composition`

A `retired-paths` entry that is a tracked file, or a folder that holds a tracked file.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `required-roots` | array of string | required | The folders that must each hold a `.ts` file; `""` is refused. |
| `retired-paths` | array of string | `[]` | Paths, files or folders, that must not exist; `""` is refused. |

The rule also reads `layout.source-root`, `layout.modules-root` and `layout.module-planes` from the
`[plugins.nestjs] options` block.

```toml
[rules]
"nestjs/owner-roots" = { level = "error", options = { required-roots = ["src/modules/domain", "src/modules/public", "src/infrastructure"], retired-paths = ["src/shared/module"] } }
```

## Example findings

```text
error nestjs/owner-roots#owner-root src/modules/public
  src/modules/public must contain source owned by the accepted architecture boundary
error nestjs/owner-roots#owner-root src/modules/catalog
  src/modules/catalog is not an accepted architecture owner root
error nestjs/owner-roots#shared-composition src/shared/module
  src/shared/module is a stale Nest composition owner and must be removed
```

## How to fix it

Add the owner's source below each required root, or drop a root the architecture no longer has.
Move a module folder into one of the module planes. Remove a retired path and move what it held to
its current owner.

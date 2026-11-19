# `nestjs/import-aliases`

Source files load other source owners through their aliases, and entities load enums only through
the enum barrels.

## What it checks and why

A service's source root is divided into owners (`src/modules`, `src/shared`, `src/config` and so
on), each reached from outside through its alias (`@shared/...`). A load through a path from the
source root, or through a relative path that climbs out of its owner, ties the file to the folder
layout instead of the owner, and breaks when either moves. An entity that loads an enum file by its
path, or its own resource's enum folder by an alias, bypasses the barrel that keeps the enums of a
resource in one place.

Every `.ts` file below `layout.source-root` is read, declaration files included and folders named in
`[languages.typescript] skip-folders` left out. Every module the file loads counts: imports and `export ... from`
(type-only ones too), `import x = require(...)`, `import(...)` types, dynamic `import(...)` and
`require(...)` with one string argument. A file without a syntax tree is skipped;
`typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Findings are located at the file and carry
no target.

### `owner-alias`

The file's owner is its first folder below the source root. A specifier is reported when it is:

- the source root itself or a path below it (`src/shared/value`);
- a relative path (`./...`, `../...`) with more than one `..` segment;
- a relative path that leaves the file's owner, when that owner is one of `layout.source-owners`.

### `entity-enum-barrel`

A file directly inside an `entity-folder` folder is an entity. Each specifier is reported when:

- it names an enum file: a relative path, a source-root path or an alias (resolved through
  `layout.path-aliases`) whose file name, without a script extension (`.ts`, `.js`, `.tsx`, `.jsx`,
  with an optional `c` or `m`), has the extension `enum-kind` (`../enum/state.enum`);
- it is not relative and names the entity's own resource's `enum-folder` (the folder beside the
  entity folder) or its `index`: the entity loads that barrel as `../enum`.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `entity-folder` | string | `"entity"` | The name of the folder whose files are entities: a single name. |
| `enum-folder` | string | `"enum"` | The name of the folder of a resource that holds its enums behind a barrel: a single name. |
| `enum-kind` | string | `"enum"` | The kind of an enum file, the extension before the script extension. |

The rule also reads `layout.source-root`, `layout.source-owners` and
`layout.path-aliases` from the `[plugins.nestjs] options` block.

```toml
[rules]
"nestjs/import-aliases" = { level = "error", options = { entity-folder = "entity", enum-folder = "enum" } }
```

## Example findings

```text
error nestjs/import-aliases#owner-alias src/modules/book/service.ts
  src/modules/book/service.ts loads ../../shared/value; use a canonical source-owner alias
error nestjs/import-aliases#entity-enum-barrel src/modules/book/entity/entity.ts
  src/modules/book/entity/entity.ts loads the enum file ../enum/state.enum; import the enum through its owner's enum barrel
```

## How to fix it

Load another owner through its alias (`@shared/value`), and keep relative paths inside the file's
own owner. In an entity, import the resource's own enums from `../enum` and another resource's enums
from that resource's enum barrel.

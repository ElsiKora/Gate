# `frontend/import-paths`

Imports out of a slice or segment go through @/, imports inside it are relative.

## What it checks and why

One spelling per target makes moves and searches mechanical. The unit of an import is a slice, a
segment of the shared layer (of a library's `src/`), the app layer, or a file directly in `src/`.
Every import, re-export, `import()` and `import("…")` type that resolves to a tracked file is read:

- `relative-exit`: a relative specifier (`./`, `../`) whose target is outside the importer's unit.
- `absolute-inside`: an `@/` specifier whose target is inside the importer's unit.
- `alias`: a specifier that is neither relative nor `@/` and resolves to a tracked file: another
  alias of the tsconfig `paths`, or a `baseUrl` import.

Imports of packages do not resolve to tracked files and are not read. A file in a folder that is no
unit (`src/modules/`) is not read for `relative-exit` and `absolute-inside`; `frontend/layout`
reports it. Target `<path>#<specifier>`.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `alias`, `relative-exit`, `absolute-inside`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/import-paths src/features/auth/ui/form.tsx:4
  src/features/auth/ui/form.tsx imports ../../../entities/user, which leaves its slice or segment by a relative path
```

## How to fix it

Name a target outside the unit as `@/<path below src/>` (`@/entities/user`), a target inside it by a
relative path, and remove other aliases from `tsconfig.json`.

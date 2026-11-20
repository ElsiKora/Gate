# `frontend/import-paths`

Imports out of a slice go through @/ and its public API, imports inside it are relative.

## What it checks and why

One spelling per target makes moves and searches mechanical, and a public API lets a slice change
inside without breaking its users. The unit of an import is a slice; the app layer and the shared
layer are one slice each, as is a library's `src/`; files directly in `src/` are a unit of their own,
and `src/main.tsx` may also import TanStack Router's `./app/route-tree.gen`, which belongs to the app
layer. The files of `src/`, of Next.js's root `app/` and Next.js's root files (`middleware.ts`,
`proxy.ts`, `instrumentation.ts`, `instrumentation-client.ts`) are read. Every import, re-export,
`import()`, `import("…")` type, call of the global `require` and `import x = require()` that resolves
to a tracked file is read:

- `relative-exit`: a relative specifier (`./`, `../`) whose target is outside the importer's unit; in
  Next.js, a relative specifier from the root `app/` whose target is outside `app/`, and one from a
  root file whose target is not at the project root (`./src/...`).
- `absolute-inside`: an `@/` specifier whose target is inside the importer's unit.
- `public-api`: an `@/` specifier that leaves the importer's unit past the target's public API. The
  public API of a slice is `@/<layer>/<slice>` (and `@/<layer>/<slice>/index.server` in Next.js); an
  entity also reaches `@/entities/<other>/@x/<itself>`. The public API of the shared layer is a segment,
  `@/shared/<segment>` (and `@/shared/<segment>/index.server` in Next.js), and for `ui` and `lib` one
  level below it (`@/shared/ui/button`). A public API is named by its folder: `@/entities/user/index`
  and `@/entities/user/index.ts` are reported with the help to write `@/entities/user`; a trailing `/`
  passes. The app layer and a library are not checked.
- `alias`: a specifier that is neither relative nor `@/` and resolves to a tracked file: another
  alias of the tsconfig `paths`, or a `baseUrl` import.

Imports of packages do not resolve to tracked files and are not read. A file in a folder that is no
unit (`src/modules/`) is not read for `relative-exit` and `absolute-inside`; `frontend/layout`
reports it. Target `<path>#<specifier>`.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `alias`, `relative-exit`, `absolute-inside`, `public-api`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/import-paths src/widgets/header/ui/header.tsx:3
  src/widgets/header/ui/header.tsx imports @/entities/user/model/store, past the public API of entities/user
```

## How to fix it

Name a target outside the unit as `@/<path below src/>` through its public API (`@/entities/user`),
export from the slice's `index.ts` what is needed, name a target inside the unit by a relative path,
and remove other aliases from `tsconfig.json`.

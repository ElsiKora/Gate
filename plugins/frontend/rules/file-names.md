# `frontend/file-names`

Files and folders under src/ are kebab-case, with route folders in their router's syntax.

## What it checks and why

One rule for every kind of file: `user-card.tsx`, `use-balance.ts`, `logo-dark.svg`. It is what
Next.js special files already require, and it cannot break on a case-insensitive file system the way a
case-only rename does. The rule reads `src/` and, in Next.js, the root `app/`. The generated client,
`src/shared/api/generated/` (a library's `src/api/generated/`), keeps the names its generator writes
(`models/UserDto.ts`, `petstore.schemas.ts`) and is not read.

- `file`: a file name whose part before the first dot is not kebab-case.
- `folder`: a folder name that is not kebab-case, reported once per folder. `@x` is allowed only
  directly in an entity slice.
- `suffix`: a dotted part other than `test`, `stories`, `module` (a CSS module), `d` (a declaration
  file) and, in Next.js, `server` (`index.server.ts`); the segment says the kind, so `name.helper.ts`
  is `lib/name.ts`. Target `<path>#<part>`.

Route folders take their router's syntax, and only there:

- `react-vite`, below `src/app/routes/`: `$param` (camel-case), `$` (splat), `_pathless`, `(group)`;
  the file `__root.tsx`; the dotted part `lazy`. TanStack Router's generated `src/app/route-tree.gen.ts`
  is not read.
- `nextjs`, below the root `app/`: `[param]`, `[...rest]`, `[[...rest]]` (camel-case parameters),
  `(group)`, `@slot`, and the intercepting prefixes `(.)`, `(..)`, `(...)`; the dotted part `alt` of
  the metadata files `opengraph-image.alt.txt` and `twitter-image.alt.txt`.

Names starting with `.` are not read.

Packs: all three; route syntax for `react-vite` and `nextjs`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `file`, `folder`, `suffix`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/file-names src/shared/ui/UserCard.tsx
  file src/shared/ui/UserCard.tsx is not kebab-case
```

## How to fix it

Rename it in kebab-case; a component file is named after its component (`user-card.tsx` for
`UserCard`). Move a kind suffix into the segment: `user.helper.ts` becomes `lib/user.ts`.

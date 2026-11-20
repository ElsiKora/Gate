# `frontend/layout`

Every file sits in the tree of the frontend standard.

## What it checks and why

One tree makes a project readable to someone who knows another. The tree is Feature-Sliced Design
with closed lists. The rule reads paths only and reports each departure once, at the folder or file
that departs:

- `source-root`: a folder outside `src/` that holds code (`.ts`, `.tsx`, `.js` and the like). Files
  at the project root (configuration), folders whose name starts with `.` (a tool's), `public/` and,
  in Next.js, the root `app/` (the App Router, `frontend/route-files`) are not read.
- `next-pages`: in Next.js, a project without `pages/README.md`, and any other file in the root
  `pages/`. The README keeps Next.js from reading `src/pages/` as the Pages Router.
- `layer`: a folder of `src/` that is not a layer: `app`, `pages`, `widgets`, `features`, `entities`,
  `shared`.
- `slice`: a file directly in a sliced layer (`pages`, `widgets`, `features`, `entities`); a slice
  folder that is not kebab-case; a slice without `index.ts`, its public API (target
  `<slice>#index.ts`); an `index.tsx` in a slice root, since a public API only re-exports.
- `group`: in `widgets`, `features` and `entities`, a folder that holds slices and no file of its own
  is a group. A file directly in a group, and a group inside a group, are reported; one level of groups
  is allowed (`features/auth/sign-in/`).
- `segment`: a file in a slice root other than its public API (`index.ts`, and `index.server.ts` in
  Next.js); a folder of a slice that is not one of `ui`, `model`, `api`, `lib`, `config`; a segment
  folder directly on a sliced layer (`features/ui/`).
- `reserved`: a folder inside a segment named after a segment (`ui/model/`, `lib/api/`); a segment
  name says the kind of a whole segment.
- `app`: a file directly in `src/app/` (other than the generated `src/app/route-tree.gen.ts` of a Vite
  app); a folder of it that is not one of its segments: `entrypoint`, `providers`, `store`, `styles`,
  `config`, `lib`, and `routes` in a Vite app. The app layer has no `ui` segment.
- `shared`: a file directly in `src/shared/`; a folder of it that is not one of `api`, `ui`, `lib`,
  `config`, `i18n`, `routes` (shared holds no slices); a segment other than `ui` and `lib` without
  `index.ts` (target `<segment>#index.ts`). In a library, a folder of `src/` that is not one of its
  segments (`api`, `config`, `i18n`, `lib`, `model`, `ui`).
- `cross-import`: an `@x` folder outside an entity, and a file in `@x` that is not an `<entity>.ts`
  directly in it.
- `root-file`: a file directly in `src/` that the pack does not allow: `main.tsx` and `vite-env.d.ts`
  for `react-vite`, `index.ts` for `react-library`, none for `nextjs`. Dotfiles are not read.

`[languages.typescript] skip-folders` are skipped.

Packs: all three; a library has no layers, so its `src/` folders are checked as segments; `next-pages` for `nextjs`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `source-root`, `next-pages`, `layer`, `slice`, `group`, `segment`, `reserved`, `app`, `shared`, `cross-import`, `root-file`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/layout src/modules
  src/modules/ is not a layer: the layers are app, pages, widgets, features, entities, shared
```

## How to fix it

Move the files into the layer and slice of their job: a business object into `entities/<name>/`, a
user action into `features/<name>/`, a block of a page into `widgets/<name>/`, a screen into
`pages/<name>/`, code that knows no business into the matching segment of `shared/`. Give each slice
an `index.ts` that re-exports what others may use.

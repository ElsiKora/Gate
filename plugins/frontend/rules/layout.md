# `frontend/layout`

Every file sits in the tree of the frontend standard.

## What it checks and why

One tree makes a project readable to someone who knows another. The rule reads paths only and
reports each departure once, at the folder or file that departs:

- `source-root`: a folder outside `src/` that holds code (`.ts`, `.tsx`, `.js` and the like). Files
  at the project root (configuration), folders whose name starts with `.` (a tool's) and `public/`
  are not read.
- `layer`: a folder of `src/` that is not a layer: `app`, `widgets`, `features`, `entities`, `shared`.
- `slice`: a file directly in `widgets/`, `features/` or `entities/`; a slice folder that is not
  kebab-case; a slice without `index.ts`, its public surface (target `<slice>#index.ts`).
- `segment`: a file in a slice root other than `index.ts`; a folder of a slice that is not one of
  `ui`, `model`, `api`, `lib`, `config`.
- `shared`: a file directly in `src/shared/`; a folder of it that is not one of `api`, `assets`,
  `config`, `i18n`, `lib`, `ui` (shared holds no slices). In a library, a folder of `src/` that is not
  one of its segments (`api`, `assets`, `config`, `i18n`, `lib`, `model`, `ui`).
- `root-file`: a file directly in `src/` that the pack does not allow: `main.tsx` and `vite-env.d.ts`
  for `react-vite`; `middleware.ts`, `proxy.ts`, `instrumentation.ts` and
  `instrumentation-client.ts` for `nextjs`; `index.ts` for `react-library`. Dotfiles are not read.

The app layer, `src/app/`, has no segments of its own; `frontend/route-files` holds its route folder.
`[languages.typescript] skip-folders` are skipped.

Packs: all three; a library has no layers, so its `src/` folders are checked as segments.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `source-root`, `layer`, `slice`, `segment`, `shared`, `root-file`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/layout src/modules
  src/modules/ is not a layer: the layers are app, widgets, features, entities, shared
```

## How to fix it

Move the files into the layer and slice of their job: a business object into `entities/<name>/`, a
user action into `features/<name>/`, a block of a page into `widgets/<name>/`, code that knows no
business into the matching segment of `shared/`.

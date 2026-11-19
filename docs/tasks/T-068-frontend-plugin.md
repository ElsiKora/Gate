# T-068: a frontend plugin, one standard for React apps and libraries

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead's decision; no code |
| Release | v0.3.0 |
| Writer | "Gate 007" |
| Reviewer | a separate review session |
| Depends on | nothing in the plugin interface: the plugin uses `gate:plugin` as it is; see "Order of work" for the one shared change |
| Touches | a new `plugins/frontend` (crate, rules, rule pages, fixtures, CHANGELOG), `Cargo.toml` (the workspace member), `docs/architecture.md` section 13, `docs/guide/plugins.md`, `CHANGELOG.md`; optionally `languages/typescript` and `plugins/nestjs/src/shared/project.rs` (see "What it reuses") |

## Goal

Frontend code is held to one standard, as `nestjs` holds services to one: Feature-Sliced Design, by
the owner's decision. A reader who knows one of our frontends finds their way in any other: the same
layers, slices and segments, the same names for the same kinds of file, imports that point one way,
one place that talks to the API, one place that reads the environment, every user-visible string
through the translation layer, and assets in two known places. ESLint and Prettier stay as they are;
this plugin checks what they do not (2.7).

- One plugin, `frontend`. Framework knowledge lives in it, never in the core.
- Three packs: `react-vite`, `nextjs` and `react-library`. Everything is shared unless a framework's
  nature forces a difference, and each difference carries its reason in the table of section 3.
- No layout options. A project picks its pack, and that is the only choice it makes.

## What was read for this design

Everything was read read-only, by listing tracked files and reading files at `HEAD`. Nothing was
checked out, fetched, installed, built or run in any of them, and no name, path or text of theirs is in
this repository. The notes that name them stay outside it.

- **The population.** 60 of our repositories hold a package that depends on React, Next.js, Vite,
  Vue or Nuxt (a read-only script over each `package.json` at `HEAD`, run 2026-11-19). About 40 of
  them are apps or libraries rather than templates, examples or tools.
- **The sample.** 28 projects, each read file by file: 13 Next.js apps, 10 React apps built with
  Vite, 4 React component libraries and one React app built with webpack. Several Vite apps are
  copies of one template; they count once each below because each is maintained on its own.
- **Left out.** The one Nuxt (Vue) app: oxc does not parse Vue single-file components, so no
  TypeScript rule can read it; it is the open item 6 below. A framework monorepo with example apps,
  whose examples are templates, not products.

The counts below come from one read-only script over the 28 projects (2026-11-19). Patterns were read
with regular expressions, so a count is "files where the pattern appears", not a parse.

## 1. What diverges today

| # | Subject | What the 28 projects do | Count |
| --- | --- | --- | --- |
| 1 | Language | TypeScript; JavaScript (`.js`, `.jsx`) | 23; 5 |
| 2 | Source root | everything under `src/`; code at the project root (`app/`, `components/`, `shared/` beside `package.json`) | 25; 3 |
| 3 | Layout | Feature-Sliced layers (`app`, `pages`, `widgets`, `features`, `entities`, `shared`); a flat `components/` `lib/` `hooks/`; `modules/` `helpers/` `stores/` `components/`; other | 9; 10; 5; 4 |
| 4 | The page layer in Next.js | route files compose the page; a Feature-Sliced `pages` layer renamed to dodge Next's reserved `pages/` folder; a separate `src/pages` beside the root `app/` | in the Feature-Sliced Next apps, all three forms occur |
| 5 | Component file names | PascalCase is the majority; kebab-case is the majority; two or more styles with three files each | 17; 11; 9 |
| 6 | Hook file names | `useThing.ts`; `use-thing.ts`; both; no hook files | 12; 7; 1; 8 |
| 7 | Kind suffixes in file names (`name.helper.ts`, `name.service.ts`, `name.type.ts`, `Name.component.tsx`) | used; not used | 10; 18 |
| 8 | Exports of components | mostly `export default`; mostly named | 13; 15 |
| 9 | Import aliases | `@/` to `src/` only; `@/` to `src/` plus more aliases (`@icons`, `@ui`, `@config`, `@public`, ...); `@/` to the project root; none | 10; 12; 3; 3 |
| 10 | Imports that climb two or more folders (`../../`) | in 11 or more files; in 1 to 10; none | 12; 6; 10 |
| 11 | Routing | Next.js; React Router; TanStack Router; none seen | 12; 8; 3; 5 |
| 12 | HTTP | `fetch` only; axios only; both; neither seen. A query cache on top (TanStack Query, SWR) | 11; 5; 4; 8. 11 |
| 13 | Raw HTTP calls outside an API folder | none; in 1 to 3 files; in 4 or more (components, route files, hooks) | 12; 8; 8 |
| 14 | Environment reads in source | in one module; in 2 or 3 folders; in 4 or more; none | 4; 9; 5; 10 |
| 15 | Translation | a translation library (three different ones); none | 4; 24 |
| 16 | Literal text in JSX (a capitalised phrase between tags) | in 6 or more files; in 1 to 5; none | 11; 11; 6 |
| 17 | State | no store library; one of four store libraries | 18; 10 (no project uses two) |
| 18 | Styles | Tailwind; Sass; CSS modules beside components; plain CSS | 15; 9; 5; 2 (some use two) |
| 19 | Assets (3 or more files in a folder) | `public/` only; `src/assets/`; `src/shared/assets/` (beside `public/`); elsewhere (brand folders, feature folders, `src/icons/`, `src/public/`) | 12; 6; 6; 4 |
| 20 | Tests and stories | none; tests; stories; both | 20; 3; 3; 2 |
| 23 | The shared ESLint config (25 read; the three copies of one template not counted again) | used; another config; none. Of those that use it, with its Feature-Sliced rules on (`withFsd`); with its translation rule on (`withI18next`) | 8; 12; 5. 2; 3 |

**The reference.** The owner named a young Vite app as the reference: it has Feature-Sliced layers
`app`, `pages`, `widgets` and `shared` (no `features` or `entities` yet), axios only in `shared/api`,
TanStack Router, and the shared ESLint config with its Feature-Sliced and translation rules. Against
the standard below, it differs in its route files, which sit directly in `pages/` with no slice; in a
`shared/hooks` folder, which is not a segment; in PascalCase files and slice folders
(`widgets/Header`, `shared/ui/Button/Button.tsx`); in a loose `src/logo.svg`; and in a tracked `.env`
(T-066's). Among the Next.js apps, the closest is one on Feature-Sliced layers with kebab-case files,
one HTTP client module and an `index.ts` on every slice; it keeps its layers at the project root and
has no `pages` layer. The farthest: the JavaScript apps of one template, with stores, helpers and
modules beside each other, PascalCase files and environment reads in six folders.

## 2. The standard: Feature-Sliced Design

The architecture is Feature-Sliced Design (FSD), as its reference pages define it
(feature-sliced.design, "Layers", "Slices and segments", "Public API"; read 2026-11-19).

### 2.1 Layers, slices, segments

```
src/
  app/        the application: entry, providers, global styles, the store root, the route files
  pages/      one slice per page
  widgets/    one slice per self-contained block of a page
  features/   one slice per user action with value
  entities/   one slice per business object
  shared/     segments only: what knows no business
```

- **Six layers**, in this order: `app`, `pages`, `widgets`, `features`, `entities`, `shared`. No
  other folder in `src/`; the deprecated `processes` layer is not one.
- **Slices.** `pages`, `widgets`, `features` and `entities` hold slices; `app` and `shared` hold
  segments directly. A slice holds segments and its public API, nothing else.
- **Segments of a slice**: `ui`, `model`, `api`, `lib`, `config`, the five FSD standardises.
- **Segments of `app`**: `routes` (the route files, section 3), `providers`, `styles`, `store`,
  `config`. No `ui`: what renders belongs to `pages` and below.
- **Segments of `shared`**: `ui`, `api`, `lib`, `config`, `i18n`, `assets`.
- **Public API.** Every slice has `index.ts`; outside code imports a slice only through it. In
  `shared`, `api`, `config` and `i18n` each have an `index.ts`, and each component folder of
  `shared/ui` and each module folder of `shared/lib` has its own, as FSD's public-API page asks for
  these "collections of unrelated things". A public API names what it exports: no `export *`.
- **Imports point down**: a module imports only slices of layers strictly below its own, never a
  sibling slice of its own layer, and only through the public API. `app` and `shared` segments import
  each other freely within their layer. Inside a slice, imports are relative; across slices they use
  the one alias `@/` for `src/`.
- **A library** (`react-library`) is a `shared` layer published on its own: `src/` holds the
  `shared` segments and `src/index.ts`, its one entry. No other layer, no routes, no environment.

### 2.2 Names

- **Every file under `src/` is kebab-case**: `user-card.tsx`, `use-balance.ts`, `invoice-api.ts`,
  `logo-dark.svg`. Folders are kebab-case too, which the shared ESLint config checks (2.7). One rule
  for every kind of file; it is what Next.js file names already are (`page.tsx`, `not-found.tsx`), and
  it cannot break on a case-insensitive file system the way a case-only rename does. PascalCase is
  today's majority for components (17 of 28) and the reference's style; question 1.
- **The only dotted parts** of a name are `.test`, `.stories`, `.module` (a CSS module), `.d` (a
  declaration file), `.gen` (a generated file, such as the router's route tree) and, in Next.js,
  `index.server.ts` (section 3). The segment says the kind, so `name.helper.ts` is `lib/name.ts`.
- **A component file** `user-card.tsx` exports `UserCard`, by name; other components it exports start
  with that name (`UserCardHeader`). No default export where the framework does not require one.
- **A hook file** `use-balance.ts` exports `useBalance`, and lives in a `model` or `lib` segment. A
  store a store library builds is a hook too (`use-cart-store.ts` exports `useCartStore`).
- **Slice names**: kebab-case; entity slices are all singular or all plural, not mixed; no suffix
  that repeats across a layer's slices (`home-page`, `about-page` in `pages`); no slice named after a
  `shared` segment (`features/i18n` beside `shared/i18n`).

### 2.3 The API

- **One module talks to the network**: `shared/api` (the client, its interceptors and any client
  generated from an OpenAPI document). `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource` and the
  HTTP packages are used nowhere else.
- **Endpoints live in `api` segments** of slices, which call `shared/api`; nothing else imports it.

### 2.4 The environment

- **One module reads the environment**: `shared/config/env.ts`, which exports typed values.
- **Each read is a literal member** (`import.meta.env.VITE_API_URL`,
  `process.env.NEXT_PUBLIC_API_URL`), never computed or destructured: the bundlers replace only
  literal reads, so any other form is `undefined` in the browser.
- Next.js adds `shared/config/server-env.ts` for values the browser must never see (section 3).

### 2.5 Text, assets, styles, tests

- **Every user-visible string goes through the translation layer** (`shared/i18n`). Messages are
  `shared/i18n/messages/<locale>.json`, one file per locale, all with the same keys. Literal JSX text
  is the shared ESLint config's rule (2.7); this plugin checks the messages. 24 of 28 projects have no
  translation library; question 2.
- **Assets** the code imports are in `shared/assets` (a library: `src/assets`); files served by a
  fixed URL (favicon, `robots.txt`, manifest, social images) are in `public/`. Nowhere else.
- **Styles**: one global stylesheet in `app/styles`; any other stylesheet is a CSS module beside the
  component of the same name (`user-card.module.css` beside `user-card.tsx`). The styling tool is
  the toolchain's choice (T-065).
- **Tests and stories** sit beside what they test: `user-card.test.tsx`, `user-card.stories.tsx`.
- **TypeScript only**: no `.js`, `.jsx`, `.mjs` or `.cjs` under `src/`.

### 2.6 Where the router sits: one answer for both frameworks

**The route files are the `routes` segment of the `app` layer, and each one only binds a URL to a
`pages` slice.** FSD's layers page lists routing among the `app` layer's jobs; a page's content, state
and data loading are its `pages` slice. A route file declares no component and holds no logic: it
re-exports or names the page slice's public API and, where its router asks, the route's options.

| | `react-vite` | `nextjs` |
| --- | --- | --- |
| The route files | `src/app/routes/`, TanStack Router file routes (`routesDirectory`); the generated tree is `src/app/routes/route-tree.gen.ts` | `src/app/` itself: Next.js looks for its router in `app/` or `src/app/` only, so the `routes` segment cannot be a subfolder; `app`'s other segments (`providers`, `styles`, `store`, `config`) are folders with no `page.tsx`, which Next.js never routes |
| A route file | `export const Route = createFileRoute("/invoices/$id")({ component: InvoicePage })`, `InvoicePage` from `@/pages/invoice` | `export { InvoicePage as default, metadata } from "@/pages/invoice"`, as FSD's Next.js guide writes it; an API route re-exports from `@/app/...` the same way |
| `src/pages` | the `pages` layer | the `pages` layer; the project root holds `pages/README.md`, one file saying why (below) |

**Why Next.js needs `pages/README.md` at the project root.** Next.js looks for `pages/` at the project
root and, when there is none, in `src/`. Without the root folder it would take FSD's `src/pages` for
its Pages Router and turn every slice into a route. With it, Next.js finds a Pages Router folder with
no page and leaves `src/pages` alone. The README is the one file the folder may hold.

**The alternative, from FSD's own Next.js guide**: Next.js's `app/` at the project root, and the FSD
layers in `src/` renamed `_app` and `_pages` in Next.js projects. It needs no README, but a Next.js app
then names two layers differently from a Vite app and from the owner's layer list, and its route files
live outside `src/`. Question 3.

### 2.7 What ESLint already checks, and Gate does not

ESLint and Prettier stay as they are; T-065 checks that every repository uses the shared ESLint config
and runs both. The shared config (version 3.10.2, read at `HEAD`) already has, behind its options:

| Option | Rules | So Gate does not check |
| --- | --- | --- |
| `withFsd` | `fsd/forbidden-imports`, `fsd/no-cross-slice-dependency`, `fsd/no-public-api-sidestep` (with `enforceShared`, no segment imports), `fsd/no-relative-imports`, `fsd/no-ui-in-business-logic`, `fsd/no-global-store-imports`, `fsd/ordered-imports` (from `eslint-plugin-fsd-lint`, which resolves tsconfig `paths`), and `no-cross-segment-reexport` | imports of a higher layer or a sibling slice, imports past a public API, relative imports across slices |
| `withI18next` | `i18next/no-literal-string` (JSX text) | literal text in JSX |
| `withCheckFile` | kebab-case for every folder under `src/` | folder names |

So the plugin holds what no per-file linter sees: the tree (which folders exist, which hold what,
which have a public API, how many there are), the import graph as a whole (which slices nobody uses),
and the placement rules of sections 2.3 to 2.5. For that split, T-065 requires `withFsd`, `withI18next`
and `withCheckFile` in every frontend's ESLint config. Today 2 of the 8 projects on the shared config
have `withFsd` on. If the lead prefers the import rules in Gate as `nestjs` has them, ESLint's rules
would have to go off for frontends; that contradicts "ESLint stays as it is", so this design does not
do it (question 4).

### 2.8 Steiger, FSD's linter: the same ground

Steiger's FSD plugin (github.com/feature-sliced/steiger, `packages/steiger-plugin-fsd`, read
2026-11-19) registers 21 rules, 17 on by default. Gate depends on none of it; each one is covered here
or by ESLint:

| Steiger rule | Default | Covered by |
| --- | --- | --- |
| `typo-in-layer-name` | on | `frontend/layout#layer` (an unknown folder of `src/`, with the nearest layer name) |
| `no-processes` | on | `frontend/layout#layer` |
| `no-layer-public-api` | on | `frontend/layout#layer-index` |
| `no-segmentless-slices` | on | `frontend/layout#segmentless` |
| `no-segments-on-sliced-layers` | on | `frontend/layout#segment-as-slice` |
| `no-reserved-folder-names` | on | `frontend/layout#reserved-name` |
| `segments-by-purpose` | on | `frontend/layout#segment` (stricter: only the segments of 2.1) |
| `no-ui-in-app` | on | `frontend/layout#segment` (`app` has no `ui` segment) |
| `excessive-slicing` | on | `frontend/layout#excessive-slicing` |
| `shared-lib-grouping` | on | `frontend/layout#shared-lib-grouping` |
| `public-api` | on | `frontend/public-api#missing` |
| `no-wildcard-exports` | off | `frontend/public-api#wildcard` (FSD's public-API page calls it bad practice) |
| `ambiguous-slice-names` | on | `frontend/slice-names#ambiguous` |
| `inconsistent-naming` | on | `frontend/slice-names#number` |
| `repetitive-naming` | on | `frontend/slice-names#repetitive` |
| `insignificant-slice` | on | `frontend/slice-usage` |
| `import-locality` | off | ESLint `fsd/no-relative-imports` for the cross-slice half; `frontend/import-locality#inside-slice` for the same-slice half, which ESLint does not check |
| `forbidden-imports` | on | ESLint `fsd/forbidden-imports` and `fsd/no-cross-slice-dependency` |
| `no-higher-level-imports` | off | ESLint `fsd/forbidden-imports` |
| `no-cross-imports` | off | ESLint `fsd/no-cross-slice-dependency` |
| `no-public-api-sidestep` | on | ESLint `fsd/no-public-api-sidestep` |

A 22nd rule, `no-file-segments`, has a page but is not registered; `frontend/layout#slice-root-file`
covers it (a file segment such as `ui.tsx` in a slice root).

## 3. The packs: what differs, and why

The plugin option `pack` names the framework. Everything in section 2 holds for every pack except the
cells below. A cell differs only because the framework itself works that way.

| Subject | `react-vite` | `nextjs` | `react-library` | Reason |
| --- | --- | --- | --- | --- |
| Entry | `index.html` loads `src/main.tsx` | Next.js starts from `src/app/layout.tsx` | `src/index.ts` | Each tool's own entry. |
| Route files | `src/app/routes/`, TanStack Router file routes, each exporting `Route` | `src/app/`, Next.js special files (`page`, `layout`, `loading`, `error`, `not-found`, `template`, `default`, `route`, `global-error`, the metadata files), each with its default export | none | 2.6. The router package is the toolchain's choice (T-065). |
| The project root | | `pages/README.md` | | 2.6. |
| Other files in `src/` root | `main.tsx`, `vite-env.d.ts` | `middleware.ts` (`proxy.ts` from Next.js 16), `instrumentation.ts` | `index.ts` | Files the framework looks up by name beside its router. |
| Route folder names | kebab-case, `$param`, `_pathless`, `(group)` | kebab-case, `[param]`, `[...rest]`, `(group)`, `@slot` | none | Each router's syntax. |
| Public API | `index.ts` | `index.ts`, and `index.server.ts` for what may run only on the server | `src/index.ts` only | FSD's Next.js guide: a server-only module exported from `index.ts` can reach a client bundle. |
| Environment | `import.meta.env.VITE_*` | `process.env.NEXT_PUBLIC_*` in `env.ts`; any key in `server-env.ts`, which starts with `import "server-only"` | no read at all | Vite exposes `VITE_` keys; Next.js inlines `NEXT_PUBLIC_` keys and keeps the rest on the server; a library is configured by its caller. |
| Client components | n/a | `"use client"` only in `ui` and `model` segments and `app/providers`; never in a route file | n/a | Only Next.js has server components; a client route file loses server data loading and `metadata`. |
| Translation binding | `react-i18next` (`useTranslation`, `t`) | `next-intl` (`useTranslations`, `getTranslations`, `t`) | `i18n` messages taken through props | `messages` must know how `t` is bound; next-intl is the one that works with server components. The package is T-065's. |
| Layers | the six | the six | `shared` segments only | A library has no pages, flows or business objects. |

## 4. The plugin

```toml
[plugins.frontend]
source = "ElsiKora/Gate:frontend@0.3.0"
options = { pack = "nextjs" }
```

- `pack` is required: `react-vite`, `nextjs` or `react-library`. A missing or unknown pack is a
  configuration error (exit 2) that lists the three. It is never inferred: a plugin cannot read
  another plugin's settings, and `package.json` is the toolchain's file.
- No layout option: the standard is the layout. A departure is an exception with its reason, and
  exceptions only shrink.
- A rule that has no meaning for a pack reports nothing there, and its page says for which packs it
  runs, so one preset serves every pack.
- One preset, `recommended`: every rule at `error`.

### What it reuses

- **`gate-typescript` (`languages/typescript`)**, as `nestjs` uses it: one parse per file and call
  (oxc reads `.tsx`), `modules::module_loads` for every import, re-export, `require` and `import()`,
  the resolver for relative and `@/` specifiers (from the tsconfig `paths` that `[languages.typescript]`
  names), `binding::ImportBindings` to know that an expression is `axios.get` or `useTranslations` of
  a package, `value` to read a literal message key, and the oxc semantic model for global references
  (`fetch`, `process.env`, `import.meta.env`).
- **The SDK**: `Tree`, typed options, the call state that lets the rules of one call share one parse.
- **The per-call sources** (`plugins/nestjs/src/shared/project.rs`: the tracked files read once per
  call and one TypeScript view). Recommendation: a first commit moves them into `gate-typescript` and
  `nestjs` uses them from there, unchanged in behavior (its tests prove it), so two plugins do not
  carry two copies. The alternative is a copy. Question 7.
- Nothing in the core changes, and the core names no framework.

## 5. Rules

Scope: project. Language: `typescript`. Reads history: no. Paths are relative to the project root.
Every finding that embeds a path or a name carries a key, so a reworded message keeps its baseline.

| Rule | What it reports | Why | Aspects | Options (default) |
| --- | --- | --- | --- | --- |
| `frontend/layout` | `source-root`: code outside `src/` (root configuration files are T-065's). `layer`: a folder of `src/` that is not a layer, with the nearest layer name. `layer-index`: an index file at a layer's top (`app` exempt). `slice-root-file`: a file in a slice other than its public API. `segmentless`: a slice with no segment. `segment-as-slice`: a segment name directly in a sliced layer. `segment`: a segment not in the list of its layer (2.1). `reserved-name`: a segment name used for a folder inside a segment. `excessive-slicing`: more slices in a layer or group than the limit. `shared-lib-grouping`: more ungrouped modules in `shared/lib` than the limit. `root-file`: a file in `src/` root the pack does not allow | The tree is the architecture; a reader finds a job by its folder. | the eleven named | `max-slices` (20), `max-shared-lib-modules` (15), Steiger's thresholds |
| `frontend/public-api` | `missing`: a slice, a public `shared` segment or a `shared/ui` or `shared/lib` folder without its `index.ts`. `wildcard`: `export *` in a public API | Outside code may use only what a slice names. | `missing`, `wildcard` | none |
| `frontend/slice-names` | `ambiguous`: a slice named like a `shared` segment. `number`: entity slices both singular and plural. `repetitive`: a suffix shared by most slices of a layer | Names that cannot be confused. | `ambiguous`, `number`, `repetitive` | none |
| `frontend/slice-usage` | `unused`: a slice nothing imports. `single-use`: a slice one other slice imports, which should hold it (`pages` slices and slices only `app` imports are exempt) | A slice is worth its folder only when it is shared; reads the whole import graph, which a per-file linter cannot. | `unused`, `single-use` | none |
| `frontend/import-locality` | `inside-slice`: an import inside one slice through `@/`. `alias`: an alias other than `@/` | One spelling per target. ESLint covers the cross-slice half. | `inside-slice`, `alias` | none |
| `frontend/file-names` | a file name under `src/` that is not kebab-case, or a dotted part other than those of 2.2; route files take their router's syntax | One name per kind of file. Folders are ESLint's (2.7). | `case`, `suffix` | none |
| `frontend/typescript-sources` | a `.js`, `.jsx`, `.mjs` or `.cjs` file under `src/` | The other rules read TypeScript; a JavaScript file is unchecked code. | none | none |
| `frontend/component-names` | a `.tsx` file in a `ui` segment that does not export the component its name gives, exports another component whose name does not start with it, or has a default export its framework does not require | A file's name finds its component. | `name`, `extra`, `default-export` | none |
| `frontend/hook-names` | a `use` + capital function exported from a file not named after it; a `use-*.ts` file that does not export its hook; a hook outside a `model` or `lib` segment | Hooks are found by name and live where state and helpers live. | `file`, `export`, `placement` | none |
| `frontend/state-placement` | a store created outside a `model` segment or `app/store` | State has one home per slice, the root store one in `app`. | none | `stores`: the store constructors (`create` of zustand, `createSlice` and `configureStore` of Redux Toolkit, `atom` of Recoil and Jotai, `makeAutoObservable` of MobX) |
| `frontend/api-boundary` | `raw-call`: `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource` or an HTTP package outside `shared/api`. `client-import`: `shared/api` imported outside an `api` segment | One client holds the base URL, auth, retries and error mapping. | `raw-call`, `client-import` | `packages`: `axios`, `ky`, `ofetch`, `cross-fetch`, `node-fetch`, `isomorphic-fetch`, `undici`, `graphql-request` |
| `frontend/env-access` | `outside`: an environment read outside the pack's environment module(s). `dynamic`: a computed, destructured or spread read inside them. `prefix`: in `env.ts`, a key without the pack's public prefix. `server-guard`: a Next.js `server-env.ts` that does not start with `import "server-only"`. In a library, any read | Bundlers inline only literal reads of public keys; one module lists every key. | the four named | none |
| `frontend/messages` | `locale-keys`: locale files whose key sets differ. `empty`: an empty message. `unknown-key`: a literal key passed to `t` that the base locale lacks, as the pack binds `t` | A key missing in one language shows the key to that language's users. | the three named | `base-locale` (`"en"`) |
| `frontend/asset-placement` | an image, icon, font, audio or video file outside `shared/assets` (library: `src/assets`) and `public/`; Next.js metadata files in `src/app/` are allowed | Imported assets are hashed by the bundler; URL ones are served as they are. | none | `extensions`: `png jpg jpeg gif webp avif svg ico woff woff2 ttf otf mp3 wav mp4 webm` |
| `frontend/style-files` | `global`: a global stylesheet outside `app/styles`, or a second one. `orphan`: a `<name>.module.<ext>` with no `<name>.tsx` beside it. `import`: a non-module stylesheet imported outside `app` | Global styles in one place; component styles travel with their component. | the three named | none |
| `frontend/route-files` | `foreign`: a file in the route folder that is not a route file of its router. `binding`: a route file that declares a component or imports anything but a `pages` slice's public API, `app` segments and its router. `export`: a route file without its router's export. `pages-holder`: in Next.js, a root `pages/` missing or holding more than `README.md` | A route file only binds a URL to a page (2.6). | the four named | none |
| `frontend/client-boundary` | Next.js only. `route-file`: `"use client"` in a route file. `placement`: `"use client"` outside `ui` and `model` segments and `app/providers` | A route file stays a server component so its data loads on the server and it can export `metadata`. | `route-file`, `placement` | none |
| `frontend/colocated-files` | a `.test` or `.stories` file without a source file of the same base name in its folder | Tests and stories move with their code. | none | none |

Each rule gets a page under `plugins/frontend/rules/` in the form of the `nestjs` pages: what it
checks and why, scope, language, aspects, target grammar, options, an example finding, how to fix it,
and for which packs it runs.

## 6. The other designs

- **T-062 (docs, the `frontend` kind).** `frontend` reads nothing under `docs/`, and `docs/layout`
  reads nothing under `src/`. The frontend kind's architecture sections (System shape, Routing,
  State, Rendering, API access) match: Routing is 2.6, State is the `model` segments, API access is
  `shared/api`; `docs/modules/` holds a chapter per slice. T-062 lists libraries as not covered;
  `react-library` is the same gap on this side.
- **T-065 (toolchain).** T-065 owns `package.json`, `tsconfig.json`, `vite.config.*`,
  `next.config.*`, ESLint and Prettier. `frontend` needs from it: the shared ESLint config with
  `withFsd`, `withI18next` and `withCheckFile` on in every frontend (2.7), and the reference turns
  `i18next/no-literal-string` off locally, which T-065 decides about; the tsconfig `paths` entry
  `"@/*": ["./src/*"]` and the same alias in `vite.config.*`; TanStack Router's `routesDirectory`
  set to `src/app/routes`; one package per job in `package.json` (one HTTP client, one store library,
  one router, one translation library, one styling tool).
- **T-066 (docker and hygiene).** T-066 owns tracked files that must not be tracked (four projects,
  the reference among them, track a `.env` other than an example) and secret shapes. `frontend` owns
  only where source reads the environment. If T-066 lists environment keys, `shared/config/env.ts` is
  the one list in a frontend; question 8.
- **`nestjs`.** Nothing shared at run time.

## 7. Tests

Native `gate-test` fixtures per rule, each a synthetic tree written for the test, with a `pass` case
and one `fail` case per aspect. Each case is one that would let a broken change through or block a
good one.

| Rule | Pass | Fail |
| --- | --- | --- |
| `layout` | the full tree of 2.1 for each pack; a group folder of slices in `features` | `src/components/`; `src/processes/`; `src/featurs/` (reported with "features"); `src/features/index.ts`; `features/auth/form.tsx`; a slice with only `index.ts`; `entities/api/`; `features/auth/hooks/`; `ui/` in `app`; `shared/ui/button/ui/`; 21 slices in `entities`; 16 files in `shared/lib`; `src/router.tsx` in `nextjs` |
| `public-api` | `index.ts` in every slice and public `shared` folder; `index.server.ts` beside it in Next.js | a slice without `index.ts`; `shared/ui/button/` without one; `export * from "./ui/card"`; `index.server.ts` in a Vite app |
| `slice-names` | `entities/user`, `entities/invoice` | `features/i18n` beside `shared/i18n`; `entities/users` beside `entities/invoice`; `pages/home-page`, `pages/about-page` |
| `slice-usage` | a feature imported by two widgets; a page imported only by its route file | an entity nothing imports; a feature only one widget imports; type-only imports count as uses |
| `import-locality` | `./ui/card` inside a slice; `@/entities/user` from a feature | `@/features/auth/ui/form` from inside `features/auth`; `@icons/x` |
| `file-names` | kebab names; `$id.tsx` and `__root.tsx` in a Vite route folder; `route-tree.gen.ts` | `UserCard.tsx`; `userCard.ts`; `user.helper.ts` |
| `typescript-sources` | `.ts`, `.tsx`, `.d.ts`; `eslint.config.mjs` at the root | `src/shared/lib/format.js` |
| `component-names` | `user-card.tsx` exporting `UserCard` and `UserCardHeader`; a Next `page.tsx` with a default export | `user-card.tsx` exporting `Card`; exporting `Avatar` too; `export default UserCard` in a `ui` segment |
| `hook-names` | `model/use-balance.ts` exporting `useBalance`; `shared/lib/use-media-query/use-media-query.ts` | `useBalance` from `model/balance.ts`; `use-balance.ts` exporting `getBalance`; a hook in `ui/` |
| `state-placement` | zustand `create` in `entities/cart/model/use-cart-store.ts`; `configureStore` in `app/store` | `create` in `ui/`; a store in `shared/lib`; `import { create as make }` is still found |
| `api-boundary` | `fetch` in `shared/api/client.ts`; `entities/user/api/user-api.ts` importing `@/shared/api` | `fetch`, `window.fetch`, `globalThis.fetch` in a component; `axios` default and named imports in a hook; `@/shared/api` imported from `ui/`; a local function named `fetch` is not reported |
| `env-access` | literal reads in `env.ts` per pack; `server-env.ts` starting with `import "server-only"` | a read in a component; `process.env[key]`; `const { A } = import.meta.env`; a key without `VITE_` in Vite; a non-public key in Next `env.ts`; `server-env.ts` without the guard; any read in a library |
| `messages` | `en.json` and `de.json` with the same nested keys; `t("a.b")` present in `en` | a key only in `de`; an empty string; `t("a.c")` absent from `en`, bound per pack (`useTranslations("a")` then `t("c")` in Next) |
| `asset-placement` | `shared/assets/logo.svg`; `public/favicon.ico`; Next `src/app/icon.png` | `features/auth/ui/logo.png`; `src/logo.svg` |
| `style-files` | `app/styles/global.css`; `user-card.module.css` beside `user-card.tsx` | a second global; `card.module.css` alone; `import "./card.css"` in a widget |
| `route-files` | a Vite route naming `UserPage` from `@/pages/user`; a Next `page.tsx` re-exporting `@/pages/user`; root `pages/README.md` | a component declared in a route file; a route file importing `@/widgets/header`; `src/app/(dashboard)/table.tsx`; a TanStack route without `Route`; Next without the root `pages/`; a page in the root `pages/` |
| `client-boundary` | `"use client"` in `ui/` and in `app/providers` | `"use client"` in `page.tsx`, `layout.tsx` and `shared/lib/format.ts`; nothing is reported for `react-vite` |
| `colocated-files` | `card.test.tsx` beside `card.tsx` | `card.stories.tsx` alone |

Plus: the configuration error for a missing and an unknown `pack`; a rule run with options of a wrong
type; one end-to-end check through the built component on a synthetic tree of each pack, run through
the lane like every build.

## Order of work, after approval

1. If question 7 is "move": the per-call sources move into `gate-typescript`; `nestjs` uses them; its
   tests pass unchanged. Ordered with the writers who touch `languages/typescript` at the time.
2. The crate and options; the rules that read paths only (`layout`, `public-api` minus `wildcard`,
   `slice-names`, `file-names`, `typescript-sources`, `asset-placement`, `colocated-files`), with
   fixtures and pages.
3. The parsing rules: `public-api#wildcard`, `slice-usage`, `import-locality`, names, state, API,
   environment, messages, styles, routes, client boundary. One lane round at each green checkpoint.
4. `docs/architecture.md` section 13 and `docs/guide/plugins.md` name the plugin; `CHANGELOG.md`
   (0.3.0, Added).

## Open, for the owner

1. **Kebab-case files** (2.2) where PascalCase is today's majority (17 of 28) and the reference's style.
2. **Translation in every app** (2.5), where 24 of 28 have none.
3. **Where Next.js's router sits** (2.6): `src/app` with a root `pages/README.md`, the same layer names
   in both frameworks (recommended); or FSD's guide, root `app/` and `_app` and `_pages` in Next.js.
4. **Import rules stay in ESLint** (2.7): Gate checks the tree and the whole graph, ESLint each import;
   T-065 requires `withFsd`, `withI18next` and `withCheckFile` in frontends.
5. **TanStack Router file routes for Vite apps** (section 3), where React Router is in 8 projects.
6. **The Nuxt (Vue) app** stays outside this plugin: no parser here reads Vue files.
7. **Moving the per-call sources** from `nestjs` into `gate-typescript` (section 4), or a copy.
8. **Environment keys** (section 6): whether T-066 lists them from `env.ts`.
9. **Server route files in Next.js** (`client-boundary#route-file`): 10 of the 12 Next.js apps read
   have client route files today.

## Done when

- The lead decides on the design and the owner on the open questions. Then the code, with one lane
  round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the release build
  exit 0, and the leak scan prints nothing.
- Do not edit `docs/plan.md` unless the lead asks.

## Resume point

2026-11-19: first design at `6ebd2b0`. Revised on the owner's direction: Feature-Sliced Design with
its six layers and a `pages` layer, the owner's reference app read, Steiger's rules covered one by one,
the router placed the same way in both frameworks, and the import and literal-text rules left to the
shared ESLint config. Waiting for the lead's decision before any code.

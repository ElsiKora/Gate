# T-068: a frontend plugin, one standard for React apps and libraries

| Field | Value |
| --- | --- |
| Status | approved for code (design at `6ebd2b0`); the code is in progress |
| Release | v0.3.0 |
| Writer | "Gate 007" |
| Reviewer | a separate review session |
| Depends on | nothing in the plugin interface: the plugin uses `gate:plugin` as it is; see "Order of work" for the one shared change |
| Touches | a new `plugins/frontend` (crate, rules, rule pages, fixtures, CHANGELOG), `Cargo.toml` (the workspace member), `docs/architecture.md` section 13, `docs/guide/plugins.md`, `CHANGELOG.md`; optionally `languages/typescript` and `plugins/nestjs/src/shared/project.rs` (see "What it reuses") |

## Goal

Frontend code is held to one standard, as `nestjs` holds services to one. A reader who knows one of
our frontends finds their way in any other: the same folders for the same jobs, the same names for the
same kinds of file, imports that point one way, one place that talks to the API, one place that reads
the environment, every user-visible string through the translation layer, and assets in two known
places.

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
| 21 | Tracked environment files that are not examples (`.env`, `.env.local`, `.env.dev`) | yes; no | 4; 24 |
| 22 | `"use client"` in Next.js route files (`page.tsx`, `layout.tsx`, up to 60 read per app) | in at least one; in none. Across the 12 apps with route files, 172 of 269 read are client files | 10; 2 |

**Closest to the standard below.** A Next.js app on Feature-Sliced layers with kebab-case files, one
HTTP client module that the `api` segments of its entities call, all but one environment read in one
module, and every slice behind an `index.ts`; it differs in keeping its layers at the project root
instead of `src/`, in a second global stylesheet, in client route files and in literal text. A Vite app on the same layers with axios only in `shared/api` and
TanStack Router. The farthest: the JavaScript apps of one template, with stores, helpers and modules beside
each other, PascalCase files and environment reads in six folders.

## 2. The standard

### 2.1 The tree

Every app keeps its code under `src/`, in Feature-Sliced layers. A layer is a folder of `src/`; it
imports only from layers below it.

```
src/
  app/           the application: entry, providers, global styles, the router and its route files
  widgets/       self-contained blocks of a page (a header, a dashboard panel): one slice per block
  features/      user actions with value (sign in, switch balance): one slice per action
  entities/      business objects (user, invoice): one slice per object
  shared/        what knows no business: segments only, no slices
    api/         the HTTP client: the one module that calls the network
    config/      env.ts: the one module that reads the environment
    i18n/        the translation setup and messages/<locale>.json
    assets/      images, icons and fonts the code imports
    ui/          the design-system components
    lib/         helpers and hooks that know no business
```

- **A slice** is a kebab-case folder of `widgets/`, `features/` or `entities/`. Its `index.ts` is its
  public surface. Its code sits in segments: `ui/` (components), `model/` (state, hooks, types,
  business logic), `api/` (the requests of this slice, through `shared/api`), `lib/` (helpers),
  `config/` (constants). No other folder and no other file sits in a slice.
- **No `pages` layer.** A route file is the page: it reads its parameters and composes widgets. This
  removes the one place where Next.js and Feature-Sliced Design collide (`pages/` is reserved by
  Next.js), the same way in both packs, without a renamed layer or an empty decoy folder.
- **No `processes` layer.** Feature-Sliced Design has deprecated it; one project used it.
- **A library** (`react-library`) is a `shared` layer published on its own: `src/` holds the shared
  segments (`ui/`, `lib/`, `model/`, `api/`, `config/`, `i18n/`, `assets/`) and `src/index.ts`, its
  one entry. It has no layers, no routes and reads no environment.

### 2.2 Names

- **Every file and folder under `src/` is kebab-case**, code, styles and assets alike:
  `user-card.tsx`, `use-balance.ts`, `invoice-api.ts`, `logo-dark.svg`. One rule for every kind of
  file; it is what Next.js and the router files already require (`page.tsx`, `not-found.tsx`), what
  the component generator of the design-system kit writes, and it cannot break on a case-insensitive
  file system the way a case-only rename of `UserCard.tsx` does. PascalCase is today's majority for
  components (17 of 28); this is the owner's call, question 1.
- **The only dotted parts** of a name are `.test`, `.stories`, `.module` (a CSS module) and `.d`
  (a declaration file). The segment says the kind, so `name.helper.ts` is `lib/name.ts` and
  `name.service.ts` is `api/name.ts`.
- **A component file** `user-card.tsx` exports `UserCard`, by name. Other components it exports start
  with that name (`UserCardHeader`). No default export, except where a framework requires one
  (section 3).
- **A hook file** `use-balance.ts` exports `useBalance`; a function named `use` and a capital letter
  lives in a file of its own name. A store built by a store library is a hook too
  (`use-cart-store.ts` exports `useCartStore`).

### 2.3 Imports

- **One alias, `@/`, for `src/`.** An import that leaves its slice (or its shared segment) names the
  target from the root: `@/entities/user`, `@/shared/api`. An import inside a slice is relative and
  never climbs out of it. No other alias.
- **Layers point down**: `app` > `widgets` > `features` > `entities` > `shared`. A slice imports no
  other slice of its own layer. `shared` imports no layer.
- **Slices are reached through their `index.ts`.** That is `typescript/barrel-imports` (it already
  reports an import past a barrel and a name a barrel does not export), so the frontend plugin does
  not check it again; the frontend preset's page tells a project to enable it.

### 2.4 The API

- **One module talks to the network**: `src/shared/api/` (the client, its interceptors and, when a
  project generates its client from an OpenAPI document, the generated code under it). `fetch`,
  `XMLHttpRequest`, `WebSocket`, `EventSource` and the HTTP packages are used nowhere else.
- **Endpoints live in `api` segments** of slices, which call the client; components and hooks call
  those, never the client.

### 2.5 The environment

- **One module reads the environment**: `src/shared/config/env.ts`, which exports typed values.
- **Each read is a literal member** (`import.meta.env.VITE_API_URL`, `process.env.NEXT_PUBLIC_API_URL`),
  never computed or destructured: the bundlers replace only literal reads, so any other form is
  `undefined` in the browser.
- Next.js adds `src/shared/config/server-env.ts` for values the browser must never see (section 3).

### 2.6 Text, assets, styles, tests

- **Every user-visible string goes through the translation layer** (`src/shared/i18n/`), including
  in an app with one language today: the strings are then in one place, and a second language is a
  file. Messages are `src/shared/i18n/messages/<locale>.json`, one file per locale, all with the same
  keys. 24 of 28 projects have no translation library; this is the owner's call, question 2.
- **Assets** the code imports are under `src/shared/assets/` (a library: `src/assets/`); files served
  by a fixed URL (favicon, `robots.txt`, manifest, social images) are under `public/`. No asset
  anywhere else.
- **Styles**: one global stylesheet in `src/app/`; any other stylesheet is a CSS module beside the
  component of the same name (`user-card.module.css` beside `user-card.tsx`). Which styling tool
  (Tailwind, Sass) is the project's choice; `frontend/packages` holds it to one.
- **Tests and stories** sit beside what they test: `user-card.test.tsx` and `user-card.stories.tsx`
  beside `user-card.tsx`.
- **TypeScript only**: no `.js`, `.jsx`, `.mjs` or `.cjs` under `src/`.

## 3. The packs: what differs, and why

The plugin option `pack` names the framework. Everything in section 2 holds for every pack except the
cells below. A cell differs only because the framework itself works that way.

| Subject | `react-vite` | `nextjs` | `react-library` | Reason |
| --- | --- | --- | --- | --- |
| Entry | `index.html` loads `src/main.tsx` | none of the app's own; Next.js starts from `src/app/layout.tsx` | `src/index.ts` | Each tool's own entry. |
| Route files | `src/app/routes/`, TanStack Router file routes (`__root.tsx`, `index.tsx`, `$id.tsx`, `route.tsx`), each exporting `Route` | `src/app/` itself, Next.js special files (`page`, `layout`, `loading`, `error`, `not-found`, `template`, `default`, `route`, `global-error` and the metadata files), each with its default export | none | Next.js routes from `src/app/` and requires default exports; a Vite app picks its router, and file routes are the form that makes the route folder look like Next's. `frontend/route-config` checks the generator's folders, `frontend/packages` the router package. |
| Other files allowed in `src/` root | `main.tsx`, `vite-env.d.ts` | `middleware.ts` (or `proxy.ts` from Next.js 16), `instrumentation.ts` | `index.ts` | Files the framework looks up by name. |
| Folder names in the route folder | kebab-case, `$param`, `_pathless`, `(group)` | kebab-case, `[param]`, `[...rest]`, `(group)`, `@slot` | none | Each router's syntax. |
| Environment syntax | `import.meta.env.VITE_*` | `process.env.NEXT_PUBLIC_*` in `env.ts`; any key in `server-env.ts`, which starts with `import "server-only"` | no read at all | Vite exposes `VITE_` keys through `import.meta.env`; Next.js inlines `NEXT_PUBLIC_` keys and keeps the rest on the server. A library is configured by its caller. |
| Client components | n/a | `"use client"` only in `ui` and `model` segments and `src/app/` providers; never in a route file | n/a | Only Next.js has server components; a client route file loses server data loading and `metadata`. |
| Translation binding | `react-i18next` (`useTranslation`, `t`) | `next-intl` (`useTranslations`, `getTranslations`, `t`) | the library's own `i18n/` messages, taken through its props | The rule that checks message keys must know how `t` is bound; each framework has one library that works with its rendering (next-intl with server components). `frontend/packages` checks that it is the one installed. |
| Layers | `app`, `widgets`, `features`, `entities`, `shared` | the same | shared segments only | A library has no pages, flows or business objects of an app. |

## 4. The plugin

```toml
[plugins.frontend]
source = "ElsiKora/Gate:frontend@0.3.0"
options = { pack = "nextjs" }
```

- `pack` is required: `react-vite`, `nextjs` or `react-library`. A missing or unknown pack is a
  configuration error (exit 2) that lists the three. It is never inferred: a plugin cannot read
  another plugin's settings, and `package.json` is the toolchain's file.
- No layout option: the standard is the layout. A project that departs from it records each
  departure as an exception with its reason, and the exceptions only shrink.
- Rules that have no meaning for a pack (route files in a library, client components outside
  Next.js) report nothing there; each rule's page says for which packs it runs, so each preset
  serves every pack.
- Two presets: `recommended`, every rule at `error`, which a project that meets the standard
  extends; and `adoption`, every rule at `warning`, which an existing project extends while it
  moves to the standard, so it sees every departure without a failing gate.

### What it reuses

- **`gate-typescript` (`languages/typescript`)**, as `nestjs` uses it: one parse per file and call
  (oxc reads `.tsx` with JSX), `modules::module_loads` for every import, re-export, `require` and
  `import()`, the resolver for relative and `@/` specifiers (from the tsconfig `paths` that
  `[languages.typescript]` names), `binding::ImportBindings` to know that an expression is `axios.get`
  or `useTranslations` of a given package, `value` to read a literal message key, and the oxc
  semantic model for global references (`fetch`, `process.env`, `import.meta.env`).
- **The SDK**: `Tree`, typed options, `NEEDS`, the call state that lets the rules of one call share
  one parse.
- **The per-call sources** (`plugins/nestjs/src/shared/project.rs`: the tracked files read once per
  call and one TypeScript view) are what `frontend` needs too. Recommendation: a first commit moves
  them into `gate-typescript` as a public type and `nestjs` uses it from there, with no change of
  behavior (its tests prove that), so the two plugins do not carry two copies. The alternative is a
  copy in `frontend`. Question 7.
- **`typescript/barrel-imports`** checks slice public surfaces (2.3); `frontend` does not.
- Nothing in the core changes, and the core names no framework.

## 5. Rules

Scope: project. Language: `typescript`. Reads history: no. Paths are relative to the project root.
Every finding that embeds a path or a name carries a key, so a reworded message keeps its baseline.

| Rule | What it reports | Why | Aspects | Options (default) |
| --- | --- | --- | --- | --- |
| `frontend/layout` | a file outside the tree of 2.1: code outside `src/` (root configuration files are T-065's), an unknown top-level folder of `src/`, a slice folder that is not kebab-case or has no `index.ts`, a folder or file in a slice that is not a segment, a slice in `shared/`, a file in `src/` root that the pack does not allow | One tree is what makes a project readable to someone who knows another. | `source-root`, `layer`, `slice`, `segment`, `shared`, `root-file` | none |
| `frontend/file-names` | a file or folder name under `src/` that is not kebab-case, or a dotted part other than `test`, `stories`, `module`, `d`; route folders take their router's syntax (section 3) | One name per kind of file, and no rename that differs only in case. | `file`, `folder`, `suffix` | none |
| `frontend/typescript-sources` | a `.js`, `.jsx`, `.mjs` or `.cjs` file under `src/` | The other rules read TypeScript; a JavaScript file is unchecked code. | none | none |
| `frontend/import-direction` | an import of a higher layer, or of another slice of the same layer, or of any layer from `shared` (libraries: none) | Layers that point one way are what keep a slice replaceable. | `layer`, `cross-slice` | none |
| `frontend/import-paths` | an alias other than `@/`; an import that leaves its slice or shared segment by a relative path; an import inside one slice through `@/` | One spelling per target makes moves and searches mechanical. | `alias`, `relative-exit`, `absolute-inside` | none |
| `frontend/component-names` | a `.tsx` file in a `ui` segment that does not export the component its name gives, exports another component whose name does not start with it, or has a default export where its framework does not require one | A file's name finds its component, and the other way round. | `name`, `extra`, `default-export` | none |
| `frontend/hook-names` | a `use` + capital function exported from a file not named after it, a `use-*.ts` file that does not export its hook, a hook outside a `model` or `lib` segment or `src/app/` | Hooks are found by name and live where state and helpers live. | `file`, `export`, `placement` | none |
| `frontend/state-placement` | a store created (`create` of zustand, `createSlice` and `configureStore` of Redux Toolkit, `atom` of Recoil or Jotai, `makeAutoObservable` of MobX) outside a `model` segment or `src/app/` | State has one home per slice; the root store is the app's. | none | `stores`: the package and export of each store constructor, the five above |
| `frontend/api-boundary` | `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource` or an HTTP package used outside `src/shared/api/`; `src/shared/api` imported outside an `api` segment and `src/app/` | One client holds the base URL, the auth header, retries and error mapping; a raw call skips all of them. | `raw-call`, `client-import` | `packages`: `axios`, `ky`, `ofetch`, `cross-fetch`, `node-fetch`, `isomorphic-fetch`, `undici`, `graphql-request`; `globals`: the four above |
| `frontend/env-access` | `import.meta.env` or `process.env` read outside the pack's environment module(s); a computed, destructured or spread read inside them; in `env.ts` a key without the pack's public prefix; in Next.js a `server-env.ts` that does not start with `import "server-only"`; in a library, any read | The bundlers inline only literal reads of public keys; one module is the list of every key the app needs. | `outside`, `dynamic`, `prefix`, `server-guard` | none |
| `frontend/literal-text` | JSX text with a letter, and a string literal in a user-visible attribute, outside the translation call, in a `.tsx` file | Text in code cannot be translated or reviewed in one place. | `text`, `attribute` | `attributes`: `alt`, `aria-label`, `aria-description`, `label`, `placeholder`, `title`; `ignore`: a regular expression for text that is not language (`^[\d\s.,:;%/()+-]*$`) |
| `frontend/messages` | locale files whose key sets differ; an empty message; a literal key passed to `t` that the base locale lacks, as the pack binds `t` (section 3) | A key missing in one language shows the key to that language's users. | `locale-keys`, `empty`, `unknown-key` | `base-locale` (`"en"`) |
| `frontend/asset-placement` | an image, icon, font, audio or video file outside `src/shared/assets/` (library: `src/assets/`) and `public/`; in Next.js the metadata files in `src/app/` are allowed | Two homes for assets: imported ones are hashed by the bundler, URL ones are served as they are. | none | `extensions`: `png jpg jpeg gif webp avif svg ico woff woff2 ttf otf mp3 wav mp4 webm` |
| `frontend/style-files` | a stylesheet in `src/` that is neither the one global stylesheet of `src/app/` nor a `<name>.module.<ext>` beside `<name>.tsx`; a second global stylesheet; a non-module stylesheet imported outside `src/app/` | Global styles in one place; component styles travel with their component. | `global`, `orphan`, `import` | none |
| `frontend/route-files` | in the route folder of the pack, a file that is not a route file of its router (components, hooks, helpers belong to layers); a route file without its required export | A route folder that holds only routes reads as the app's map. | `foreign`, `export` | none |
| `frontend/client-boundary` | Next.js only: `"use client"` in a route file, or in a file outside the `ui` and `model` segments and `src/app/` providers | A route file stays a server component so its data loads on the server and it can export `metadata`. | `route-file`, `placement` | none |
| `frontend/colocated-files` | a `.test` or `.stories` file without a source file of the same base name in its folder | Tests and stories move with their code. | none | none |
| `frontend/import-alias` | `tsconfig.json` (JSON with comments) without the `paths` entry `"@/*": ["./src/*"]`, or with another alias; when `vite.config.*` exists, no `@` alias to `src/` in its `resolve.alias` (object or `{ find, replacement }` form) and no `vite-tsconfig-paths` plugin | `import-paths` holds code to `@/`; this is the alias it relies on, in the type checker and in the bundler. | `tsconfig`, `vite` | none |
| `frontend/route-config` | `react-vite` only: `vite.config.*` without the `TanStack` Router plugin (`tanstackRouter` or `TanStackRouterVite` of `@tanstack/router-plugin/vite`), or with a `routesDirectory` other than `./src/app/routes` or a `generatedRouteTree` other than `./src/app/route-tree.gen.ts` (a missing option is the plugin's default, which is not ours) | The route folder of section 3 is where the generator looks only if the plugin says so. | `plugin`, `routes-directory`, `route-tree` | none |
| `frontend/packages` | `package.json` `dependencies` and `devDependencies` naming two tools of one job (HTTP client, store, router, translation, styling), or a router or translation library other than the pack's (decisions 4 and 5) | One package per job is what makes a slice read the same in every project. | `job`, `pack` | `jobs`: each job with its tools, a tool being one or more packages (emotion is `@emotion/react` and `@emotion/styled`); the defaults are on the rule's page |

Each rule gets a page under `plugins/frontend/rules/` in the form of the `nestjs` pages: what it
checks and why, scope, language, aspects, target grammar, options, an example finding, how to fix it,
and for which packs it runs.

## 6. The other designs

- **T-062 (docs, the `frontend` kind).** `frontend` reads nothing under `docs/`, and `docs/layout`
  reads nothing under `src/`. The frontend kind's architecture sections (System shape, Routing,
  State, Rendering, API access) match section 2: Routing is the route folder of section 3, State is
  the `model` segments, API access is `shared/api`; `docs/modules/` holds a chapter per slice, which
  is "a part of the code with an owner". T-062 lists libraries as not covered; `react-library` is
  the same gap on this side, so the owner may want both decided together.
- **T-065 (toolchain).** T-065 owns the generic toolchain: `package.json` scripts and engines,
  `tsconfig.json` compiler settings, `next.config.*`, ESLint and Prettier. It checks nothing this
  plugin checks. The three frontend parts of those files are this plugin's (decision 10), read
  through the plugin interface: the `@/*` alias in `tsconfig.json` and `vite.config.*`
  (`frontend/import-alias`), the `TanStack` Router plugin's folders in `vite.config.*`
  (`frontend/route-config`), and one package per job in `package.json` (`frontend/packages`).
  `node/linter` neither requires nor forbids any FSD or i18n ESLint option: boundaries, case and
  literal text are this plugin's, so each finding is reported once.
- **T-066 (docker and hygiene).** T-066 owns tracked files that must not be tracked (four projects
  track a `.env` other than an example; one tracks build output) and secret shapes. `frontend` owns
  only where source reads the environment. If T-066 adds an inventory of environment keys, it can
  read the keys from `src/shared/config/env.ts`, which this standard makes the one list; question 8.
- **`nestjs`.** Nothing shared at run time; rule ids differ (`frontend/import-direction` is about
  layers, `nestjs/import-direction` about contracts).

## 7. Tests

Native `gate-test` fixtures per rule, each a synthetic tree written for the test, with a `pass` case
and one `fail` case per aspect. Each case is one that would let a broken change through or block a
good one.

| Rule | Pass | Fail |
| --- | --- | --- |
| `layout` | the full tree of 2.1 per pack, each segment present | `components/` at the project root; `src/modules/`; slice `UserCard/`; slice without `index.ts`; `entities/user/helpers/`; `shared/user/index.ts`; `src/router.tsx` in `nextjs` |
| `file-names` | kebab names; `[id]` and `(group)` in a Next route folder; `$id.tsx` and `__root.tsx` in a Vite route folder | `UserCard.tsx`; `userCard.ts`; `user.helper.ts`; `[id]` in a Vite route folder |
| `typescript-sources` | `.ts`, `.tsx`, `.d.ts`; `eslint.config.mjs` at the root | `src/shared/lib/format.js` |
| `import-direction` | widgets importing features, entities, shared; `src/app` importing all | entities importing features; feature `a` importing feature `b`; shared importing entities; type-only imports count too |
| `import-paths` | `@/entities/user` from a feature; `./ui/card` inside a slice | `@icons/x`; `../../entities/user` from a feature; `@/features/auth/ui/form` from inside `features/auth` |
| `component-names` | `user-card.tsx` exporting `UserCard` and `UserCardHeader`; a Next `page.tsx` with a default export | `user-card.tsx` exporting `Card`; exporting `Avatar` too; `export default UserCard` in a `ui` segment |
| `hook-names` | `model/use-balance.ts` exporting `useBalance`; `shared/lib/use-media-query.ts` | `useBalance` exported from `model/balance.ts`; `use-balance.ts` exporting `getBalance`; a hook in `ui/` |
| `state-placement` | zustand `create` in `entities/cart/model/use-cart-store.ts`; `configureStore` in `src/app/store.ts` | `create` from zustand in `ui/`; a store in `shared/lib`; an aliased import (`import { create as make }`) is still found |
| `api-boundary` | `fetch` in `shared/api/client.ts`; `entities/user/api/user-api.ts` importing `@/shared/api` | `fetch` in a component; `window.fetch` and `globalThis.fetch`; `axios` default and named imports in a hook; `@/shared/api` imported from a `ui` segment; a local function named `fetch` is not reported |
| `env-access` | literal reads in `env.ts` per pack; Next `server-env.ts` starting with `import "server-only"` | a read in a component; `process.env[key]`; `const { A } = import.meta.env`; `VITE_` key missing its prefix in Vite; a non-public key in Next `env.ts`; `server-env.ts` without the guard; any read in a library |
| `literal-text` | `{t("title")}`; numbers and punctuation; `className` strings | `<h1>Welcome back</h1>`; `placeholder="Search"`; text inside a fragment |
| `messages` | `en.json` and `de.json` with the same nested keys; `t("a.b")` present in `en` | a key only in `de`; an empty string; `t("a.c")` absent from `en`, bound per pack (`useTranslations("a")` then `t("c")` in Next) |
| `asset-placement` | `src/shared/assets/logo.svg`; `public/favicon.ico`; Next `src/app/icon.png` | `src/features/auth/ui/logo.png`; `src/assets/hero.jpg` in an app |
| `style-files` | `src/app/globals.css`; `user-card.module.css` beside `user-card.tsx` | a second global; `card.module.css` with no `card.tsx`; `import "./card.css"` in a widget |
| `route-files` | the special files of each router | `src/app/(dashboard)/table.tsx` in Next; a hook in `src/app/routes/`; a TanStack route file without `Route` |
| `client-boundary` | `"use client"` in `ui/` and in `src/app/providers.tsx` | `"use client"` in `page.tsx` and `layout.tsx`; in `shared/lib/format.ts`; nothing is reported for `react-vite` |
| `colocated-files` | `card.test.tsx` beside `card.tsx` | `card.stories.tsx` alone |
| `import-alias` | `paths` `@/*` in `tsconfig.json` with comments; Vite `resolve.alias` `@` to `path.resolve(__dirname, "./src")`, and the array form; `vite-tsconfig-paths`; a library without `vite.config.*` | no `paths`; `@/*` to `./app/*`; a second alias `~/*`; a Vite config with no `@` alias |
| `route-config` | the plugin with both folders, as `tanstackRouter` and `TanStackRouterVite` | no plugin; `routesDirectory: "./src/routes"`; no `generatedRouteTree`; nothing is reported for `nextjs` and `react-library` |
| `packages` | one of each job; `i18next` beside `react-i18next`; `@emotion/react` beside `@emotion/styled` | `axios` and `ky`; `zustand` and `@reduxjs/toolkit`; `react-router-dom` in `react-vite`; `react-i18next` in `nextjs`; a custom `jobs` table |

Plus: the configuration error for a missing and an unknown `pack`; a rule run with options of a wrong
type; one end-to-end check through the built component on a synthetic tree of each pack, run through
the lane like every build.

## Order of work, after approval

1. The per-call sources move into `gate-typescript` (decision 7); `nestjs` uses them; its tests pass
   unchanged. Coordinated with the writers who touch `languages/typescript` at the time.
2. The crate, the options and the `layout`, `file-names`, `typescript-sources` rules (they read paths
   only), with their fixtures and pages.
3. The import rules, then names, state, API and environment, then text and messages, then the
   Next.js-only and placement rules. One lane round at each green checkpoint.
4. The toolchain rules of decision 10: `import-alias`, `route-config`, `packages`, their pages and
   both presets, once the lead says their code may start.
5. `docs/architecture.md` section 13 and `docs/guide/plugins.md` name the plugin; `CHANGELOG.md`
   (0.3.0, Added).

## Decided

The lead approved the design at `6ebd2b0` for code in v0.3.0 and answered the open questions:

1. **Kebab-case files**: yes, every file and folder under `src/` (2.2).
2. **Translation in every app**: yes (2.6); `literal-text` and `messages` run in every app.
3. **No `pages` layer**: yes; the route file is the page in both app packs (2.1).
4. **TanStack Router file routes** for Vite apps (section 3). A Vite app on React Router sees its
   departures through the `adoption` preset until it moves.
5. **One translation library per framework**: `react-i18next` for `react-vite` and `react-library`,
   `next-intl` for `nextjs`.
6. **Nuxt stays outside** the plugin, and the plugin's page says so; there is no Vue pack in 0.3.0.
7. **The per-call sources move** from `nestjs` into `gate-typescript` (`gate_typescript::call`) as
   the first commit of the code, before the plugin, so the writers who build on them rebase onto it.
8. **Environment keys**: T-066's environment check reads a frontend's keys from
   `src/shared/config/env.ts` and, in Next.js, `src/shared/config/server-env.ts`.
9. **Next.js route files are server components**: `"use client"` belongs in the `ui` (and `model`)
   segments; an existing app with client route files sees them through the `adoption` preset.
10. **The frontend parts of the toolchain files are this plugin's** (ruling after T-065's question):
    the `@/*` alias in `tsconfig.json` and `vite.config.*`, the `TanStack` Router folders in
    `vite.config.*`, and one package per job in `package.json`, each a rule with pass and fail
    tests and a place in both presets (`import-alias`, `route-config`, `packages`). T-065 checks
    none of them and owns the generic toolchain only.

## Done when

- The lead decides on the design and the owner on the open questions. Then the code, with one lane
  round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the release build
  exit 0, and the leak scan prints nothing.
- Do not edit `docs/plan.md` unless the lead asks.

## Resume point

2026-11-20T01:51:58+00:00: the crate, its 17 rules, their native tests, the end-to-end tests, the
plugin page, one page per rule and the changelogs are committed (`75b3253` to `787f656`), and
`plugins/frontend` is a workspace member. Nothing of the crate has compiled yet: its first lane round
is queued. Next: fix what that round reports, then the review of the order of work's last steps.

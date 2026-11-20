# T-068: a frontend plugin, one standard for React apps and libraries

| Field | Value |
| --- | --- |
| Status | approved for code (design at `6ebd2b0`); the design revised for the second FSD pass's rulings (decision 11), for the lead to read; then one code fix round |
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

The standard is Feature-Sliced Design (FSD) as its documentation describes it: layers, slices and
segments, public APIs, cross-imports, and its Next.js guide. The second independent FSD pass
(2026-11-20) checked it against the FSD documentation, Steiger 0.8.2 and fsd-lint 1.2.1. Where it is
stricter than FSD, 2.7 says where and why.

### 2.1 The tree

Every app keeps its code under `src/`, in FSD layers. A layer is a folder of `src/`; it imports only
from layers below it.

```
src/
  app/           the app layer, one slice: segments only
    entrypoint/  what starts the app: the router instance, the root render (react-vite)
    providers/   the providers that wrap the app
    store/       the root store, when the app has one
    styles/      the one global stylesheet
    routes/      the TanStack Router file routes (react-vite only), thin
    config/, lib/
  pages/         one slice per page
  widgets/       self-contained blocks of a page (a header, a dashboard panel): one slice per block
  features/      user actions with value (sign in, switch balance): one slice per action
  entities/      business objects (user, invoice): one slice per object
    <a>/@x/<b>.ts  what entity <a> offers entity <b>, and only <b>
  shared/        what knows no business, one slice: segments only
    api/         the HTTP client: the one module that calls the network
    ui/          the design-system components, and the assets they share
    lib/         helpers and hooks that know no business
    config/      env.ts: the one module that reads the environment
    i18n/        the translation setup and messages/<locale>.json
    routes/      route paths and their builders
```

- **Layers**, top to bottom: `app`, `pages`, `widgets`, `features`, `entities`, `shared`. Every app
  pack has all six names available; a layer with nothing in it is absent. No `processes` layer: FSD
  has deprecated it.
- **A slice** is a kebab-case folder of `pages/`, `widgets/`, `features/` or `entities/`. Its root
  holds its public API and segments, nothing else:
  - `index.ts`, the public API, which only re-exports;
  - in `nextjs`, also `index.server.ts`, the public API of what may run only on the server;
  - the segments `ui/` (components and the assets they import), `model/` (state, hooks, types,
    business logic), `api/` (the requests of this slice, through `shared/api`), `lib/` (helpers) and
    `config/` (constants);
  - in an entity only, `@x/` (2.3).
- **Slice groups.** In `widgets/`, `features/` and `entities/`, a folder may group slices: it has no
  `index.ts`, every child of it is a slice, and a group holds no group. `features/auth/sign-in/` is
  the slice `sign-in` in the group `auth`.
- **`app` and `shared` are one slice each**: segments only, no slices. Their segments import each
  other by relative path (`../lib/format` from `shared/ui` is fine). `app` has no `ui` segment.
- **No segment directly on a sliced layer** (`features/ui/`), and **no folder inside a segment named
  after a segment** (`features/auth/ui/lib/`); the names are Steiger's reserved list: `ui`, `api`,
  `model`, `lib`, `config`.
- **The pages layer.** A route file is thin: it reads its parameters and renders the page of
  `@/pages/<slice>`. A block that is most of one page and not reused stays in that page's slice; it
  moves to `widgets/` when a second page uses it.
- **Next.js** (`nextjs`) keeps the App Router in the root `app/`, beside `src/`, as FSD's Next.js
  guide does: route files that re-export or compose from `@/pages/...`, and Next's metadata and route
  handler files. A root `pages/` holding only a `README.md` keeps Next from reading `src/pages/` as
  the Pages Router; it is required. Next's root files (`middleware.ts` or `proxy.ts`,
  `instrumentation.ts`, `instrumentation-client.ts`) sit at the project root beside `app/`, where Next
  looks for them when the app folder is there.
- **A library** (`react-library`) is a `shared` layer published on its own: `src/` holds the
  segments `ui/`, `lib/`, `model/`, `api/`, `config/` and `i18n/`, and `src/index.ts`, its one entry.
  It has no layers, no routes and reads no environment.

### 2.2 Names

- **Every file and folder under `src/` is kebab-case**, code, styles and assets alike:
  `user-card.tsx`, `use-balance.ts`, `invoice-api.ts`, `logo-dark.svg`. One rule for every kind of
  file; it is what Next.js and the router files already require (`page.tsx`, `not-found.tsx`), what
  the component generator of the design-system kit writes, and it cannot break on a case-insensitive
  file system the way a case-only rename of `UserCard.tsx` does. The root `app/` of a Next.js app
  follows the same rule.
- **Route folders take their router's syntax** besides kebab-case (section 3). A route parameter is
  camelCase (`$invoiceId`, `[invoiceId]`): its name becomes a property of the router's `params`
  object, and camelCase is what the code then reads (`params.invoiceId`) without quoting.
- **`@x`** is a folder name only in an entity slice's root (2.3).
- **The only dotted parts** of a name are `.test`, `.stories`, `.module` (a CSS module), `.d`
  (a declaration file) and `.server` (`index.server.ts`, in `nextjs`); in a Vite route folder also
  `.lazy` (TanStack Router's lazy route files), and `src/app/route-tree.gen.ts`, the file TanStack
  Router's generator writes, keeps its name. The segment says the kind, so `name.helper.ts` is
  `lib/name.ts` and `name.service.ts` is `api/name.ts`.
- **A component file** `user-card.tsx` exports `UserCard`, by name. Other components it exports start
  with that name (`UserCardHeader`). No default export, except where a framework requires one
  (section 3).
- **A hook file** `use-balance.ts` exports `useBalance`; a function named `use` and a capital letter
  lives in a file of its own name. A store built by a store library is a hook too
  (`use-cart-store.ts` exports `useCartStore`).

### 2.3 Imports

- **One alias, `@/`, for `src/`.** An import that leaves its slice names the target from the root;
  an import inside a slice is relative and never climbs out of it. No other alias.
- **Layers point down**: `app` > `pages` > `widgets` > `features` > `entities` > `shared`; the root
  `app/` of a Next.js app is above them all. A slice imports no other slice of its own layer, except
  through `@x`.
- **A slice is reached through its public API**, which the frontend plugin checks itself, in static
  imports, `export ... from` and `import()`:
  - a slice of `pages`, `widgets`, `features` or `entities`: `@/<layer>/<slice>` (with its group, if
    any), and in `nextjs` also `@/<layer>/<slice>/index.server`;
  - `shared`: `@/shared/<segment>`; for `ui` and `lib` also `@/shared/<segment>/<name>`, one level
    down. Every other shared segment (`api`, `config`, `i18n`, `routes`) has an `index.ts`, and
    `@/shared/config/env` is a sidestep;
  - anything deeper is a sidestep.
- **`@x`, entities only.** Entity `b` may import entity `a` through `@/entities/a/@x/b`, the file
  `src/entities/a/@x/b.ts`, which re-exports what `a` offers `b`. Only `src/entities/b/` imports it;
  `@x` anywhere else is reported.

### 2.4 The API

- **One module talks to the network**: `src/shared/api/` (the client, its interceptors and, when a
  project generates its client from an OpenAPI document, the generated code under it). `fetch`,
  `XMLHttpRequest`, `WebSocket`, `EventSource` and the HTTP packages are used nowhere else.
- **Endpoints live in `api` segments** of slices, which call the client; components and hooks call
  those, never the client.

### 2.5 The environment

- **One module reads the environment**: `src/shared/config/env.ts`, which exports typed values; the
  rest of the app takes them from `@/shared/config`.
- **Each read is a literal member** (`import.meta.env.VITE_API_URL`, `process.env.NEXT_PUBLIC_API_URL`),
  never computed or destructured: the bundlers replace only literal reads, so any other form is
  `undefined` in the browser.
- Next.js adds `src/shared/config/server-env.ts` for values the browser must never see, exported from
  `src/shared/config/index.server.ts` (section 3).

### 2.6 Text, assets, styles, tests

- **Every user-visible string goes through the translation layer** (`src/shared/i18n/`), including
  in an app with one language today: the strings are then in one place, and a second language is a
  file. Messages are `src/shared/i18n/messages/<locale>.json`, one file per locale, all with the same
  keys.
- **Assets** the code imports sit in the `ui` segment of the slice that uses them, or in
  `src/shared/ui/` when several slices share them (a library: `src/ui/`). Files served by a fixed URL
  (favicon, `robots.txt`, manifest, social images) are under `public/`, and in Next.js the metadata
  files sit in the root `app/`. No asset anywhere else.
- **Styles**: one global stylesheet in `src/app/styles/`; any other stylesheet is a CSS module beside
  the component of the same name (`user-card.module.css` beside `user-card.tsx`). Which styling tool
  (Tailwind, Sass) is the project's choice; `frontend/packages` holds it to one.
- **Tests and stories** sit beside what they test: `user-card.test.tsx` and `user-card.stories.tsx`
  beside `user-card.tsx`.
- **TypeScript only**: no `.js`, `.jsx`, `.mjs` or `.cjs` under `src/`.

### 2.7 Stricter than FSD, and what is left out

Stricter, each for its reason:

- **The segment list and the slice root are closed** (2.1). FSD allows any segment name and other
  files in a slice; one closed list is what makes one tree in every project.
- **No `_app` or `_pages` rename** for Next.js, which FSD allows. The root `app/` and the root
  `pages/README.md` keep the layer names `app` and `pages` as they are, so every pack has one layer
  name for each layer.
- **`index.tsx` is not a public API**; FSD accepts it. A public API holds re-exports only, and a
  `.tsx` index invites a component into it.
- **`@x` on entities only**; FSD documents it for entities, and the standard allows nothing else.

Not adopted from Steiger and fsd-lint: their heuristic rules (insignificant slices, repetitive
naming, ambiguous slice names), wildcard re-exports and circular imports.

## 3. The packs: what differs, and why

The plugin option `pack` names the framework. Everything in section 2 holds for every pack except the
cells below. A cell differs only because the framework itself works that way.

| Subject | `react-vite` | `nextjs` | `react-library` | Reason |
| --- | --- | --- | --- | --- |
| Entry | `index.html` loads `src/main.tsx` | none of the app's own; Next.js starts from the root `app/layout.tsx` | `src/index.ts` | Each tool's own entry. |
| Route files | `src/app/routes/`, TanStack Router file routes (`__root.tsx`, `index.tsx`, `$id.tsx`, `route.tsx`, and `.lazy.tsx` files), each exporting `Route`; `src/app/route-tree.gen.ts` is the generator's | the root `app/`, Next.js special files (`page`, `layout`, `loading`, `error`, `not-found`, `template`, `default`, `route`, `global-error`, `forbidden`, `unauthorized` and the metadata files), each with its default export | none | Next.js routes from its app folder and requires default exports; FSD's Next.js guide puts that folder at the root so that `src/app` stays the app layer. A Vite app picks its router, and file routes are the form that makes the route folder look like Next's. `frontend/route-config` checks the generator's folders, `frontend/packages` the router package. |
| Files allowed in `src/` root | `main.tsx`, `vite-env.d.ts` | none | `index.ts` | Files the framework looks up by name; Next.js looks up its own (`middleware.ts` or `proxy.ts`, `instrumentation.ts`, `instrumentation-client.ts`) beside its app folder, at the project root. |
| Outside `src/` | configuration files at the project root | also the root `app/`, and the root `pages/` with only a `README.md`, which is required | configuration files at the project root | Next.js reads `src/pages/` as the Pages Router unless a root `pages/` exists. |
| Folder names in the route folder | kebab-case, `$param`, `$` (splat), `_pathless`, `(group)` | kebab-case, `[param]`, `[...rest]`, `[[...rest]]`, `(group)`, `@slot`, and intercepting routes (`(.)`, `(..)`, `(...)` before a name) | none | Each router's syntax; parameters in camelCase (2.2). |
| Public API of a slice | `index.ts` | `index.ts`, and `index.server.ts` for server-only exports | `src/index.ts` | Next.js runs some code only on the server, and a separate entry keeps it out of client bundles. |
| Environment syntax | `import.meta.env.VITE_*` | `process.env.NEXT_PUBLIC_*` in `env.ts`; any key in `server-env.ts`, which starts with `import "server-only"` | no read at all | Vite exposes `VITE_` keys through `import.meta.env`; Next.js inlines `NEXT_PUBLIC_` keys and keeps the rest on the server. A library is configured by its caller. |
| Client components | n/a | `"use client"` only in `ui` and `model` segments and `src/app/providers/`; never in a route file | n/a | Only Next.js has server components; a client route file loses server data loading and `metadata`. |
| Translation binding | `react-i18next` (`useTranslation`, `t`) | `next-intl` (`useTranslations`, `getTranslations`, `t`) | the library's own `i18n/` messages, taken through its props | The rule that checks message keys must know how `t` is bound; each framework has one library that works with its rendering (next-intl with server components). `frontend/packages` checks that it is the one installed. |
| Layers | `app`, `pages`, `widgets`, `features`, `entities`, `shared` | the same | shared segments only | A library has no pages, flows or business objects of an app. |

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
- **Slice public APIs** are checked by `frontend/import-paths` itself (2.3), in static imports,
  `export ... from` and `import()`; the opt-in `typescript/barrel-imports` is not needed for them.
- Nothing in the core changes, and the core names no framework.

## 5. Rules

Scope: project. Language: `typescript`. Reads history: no. Paths are relative to the project root.
Every finding that embeds a path or a name carries a key, so a reworded message keeps its baseline.

| Rule | What it reports | Why | Aspects | Options (default) |
| --- | --- | --- | --- | --- |
| `frontend/layout` | a file outside the tree of 2.1: code outside `src/` (root configuration files are the toolchain's; in `nextjs` the root `app/` is allowed), in `nextjs` a root `pages/` that is missing, lacks its `README.md` or holds anything else; an unknown layer of `src/`; a slice that is not kebab-case, has no `index.ts`, has `index.tsx`, or holds a file or folder that is not its public API or a segment; a group with an `index.ts`, a file, or a group inside it; a segment directly on a sliced layer; a folder inside a segment named after a segment; a `ui` segment, an unknown segment or a loose file in `src/app/`; an unknown shared segment, or a shared segment other than `ui` and `lib` without `index.ts`; `@x` other than `src/entities/<a>/@x/<b>.ts`; a file in `src/` root that the pack does not allow | One tree is what makes a project readable to someone who knows another. | `source-root`, `next-pages`, `layer`, `slice`, `group`, `segment`, `reserved`, `app`, `shared`, `cross-import`, `root-file` | none |
| `frontend/file-names` | a file or folder name under `src/` (and the root `app/` of `nextjs`) that is not kebab-case, or a dotted part other than `test`, `stories`, `module`, `d` and, in `nextjs`, `server`; route folders take their router's syntax (section 3), with `.lazy` and `route-tree.gen.ts` in Vite; `@x` only in an entity slice's root | One name per kind of file, and no rename that differs only in case. | `file`, `folder`, `suffix` | none |
| `frontend/typescript-sources` | a `.js`, `.jsx`, `.mjs` or `.cjs` file under `src/` | The other rules read TypeScript; a JavaScript file is unchecked code. | none | none |
| `frontend/import-direction` | an import of a higher layer, of another slice of the same layer other than through an entity's `@x` for the importing entity, or of any layer from `shared` (libraries: none); the root `app/` of `nextjs` may import every layer | Layers that point one way are what keep a slice replaceable. | `layer`, `cross-slice` | none |
| `frontend/import-paths` | an alias other than `@/`; an import that leaves its slice by a relative path (`app` and `shared` are one slice each); an import inside one slice through `@/`; an import from outside a slice past its public API (2.3), in static imports, `export ... from` and `import()` | One spelling per target makes moves and searches mechanical, and a public API is what lets a slice change inside. | `alias`, `relative-exit`, `absolute-inside`, `public-api` | none |
| `frontend/component-names` | a `.tsx` file in a `ui` segment that does not export the component its name gives, exports another component whose name does not start with it, or has a default export where its framework does not require one | A file's name finds its component, and the other way round. | `name`, `extra`, `default-export` | none |
| `frontend/hook-names` | a `use` + capital function exported from a file not named after it, a `use-*.ts` file that does not export its hook, a hook outside a `model` or `lib` segment and the app layer's `providers` and `store` | Hooks are found by name and live where state and helpers live. | `file`, `export`, `placement` | none |
| `frontend/state-placement` | a store created (`create` of zustand, `createSlice` and `configureStore` of Redux Toolkit, `atom` of Recoil or Jotai, `makeAutoObservable` of MobX) outside a `model` segment and `src/app/store/` | State has one home per slice; the root store is the app's. | none | `stores`: the package and export of each store constructor, the five above |
| `frontend/api-boundary` | `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource` or an HTTP package used outside `src/shared/api/`; `src/shared/api` imported outside an `api` segment and `src/app/` | One client holds the base URL, the auth header, retries and error mapping; a raw call skips all of them. | `raw-call`, `client-import` | `packages`: `axios`, `ky`, `ofetch`, `cross-fetch`, `node-fetch`, `isomorphic-fetch`, `undici`, `graphql-request`; `globals`: the four above |
| `frontend/env-access` | `import.meta.env` or `process.env` read outside the pack's environment module(s); a computed, destructured or spread read inside them; in `env.ts` a key without the pack's public prefix; in Next.js a `server-env.ts` that does not start with `import "server-only"`; in a library, any read | The bundlers inline only literal reads of public keys; one module is the list of every key the app needs. | `outside`, `dynamic`, `prefix`, `server-guard` | none |
| `frontend/literal-text` | JSX text with a letter, and a string literal in a user-visible attribute, outside the translation call, in a `.tsx` file | Text in code cannot be translated or reviewed in one place. | `text`, `attribute` | `attributes`: `alt`, `aria-label`, `aria-description`, `label`, `placeholder`, `title`; `ignore`: a regular expression for text that is not language (`^[\d\s.,:;%/()+-]*$`) |
| `frontend/messages` | locale files whose key sets differ; an empty message; a literal key passed to `t` that the base locale lacks, as the pack binds `t` (section 3) | A key missing in one language shows the key to that language's users. | `locale-keys`, `empty`, `unknown-key` | `base-locale` (`"en"`) |
| `frontend/asset-placement` | an image, icon, font, audio or video file outside a `ui` segment (a slice's, `src/shared/ui/`, a library's `src/ui/`) and `public/`; in `nextjs` the metadata files in the root `app/` are allowed | Imported assets travel with what uses them and are hashed by the bundler; URL ones are served as they are. | none | `extensions`: `png jpg jpeg gif webp avif svg ico woff woff2 ttf otf mp3 wav mp4 webm` |
| `frontend/style-files` | a stylesheet in `src/` that is neither the one global stylesheet of `src/app/styles/` nor a `<name>.module.<ext>` beside `<name>.tsx`; a second global stylesheet; a non-module stylesheet imported outside `src/app/` and the root `app/` | Global styles in one place; component styles travel with their component. | `global`, `orphan`, `import` | none |
| `frontend/route-files` | in the route folder of the pack (the root `app/` in `nextjs`, `src/app/routes/` in `react-vite`), a file that is not a route file of its router (components, hooks, helpers belong to slices); a route file without its required export | A route folder that holds only thin routes reads as the app's map. | `foreign`, `export` | none |
| `frontend/client-boundary` | Next.js only: `"use client"` in a route file of the root `app/`, or in a file outside the `ui` and `model` segments and `src/app/providers/` | A route file stays a server component so its data loads on the server and it can export `metadata`. | `route-file`, `placement` | none |
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
| `layout` | the full tree of 2.1 per pack; a slice group `features/auth/sign-in/`; `entities/user/@x/invoice.ts`; `index.server.ts` in `nextjs`; the root `app/` and `pages/README.md` in `nextjs` | `components/` at the project root; `src/modules/`; slice `UserCard/`; slice without `index.ts`; `index.tsx`; a group with `index.ts`; a group in a group; `features/ui/`; `features/auth/ui/lib/`; `src/app/ui/`; `entities/user/helpers/`; `shared/user/index.ts`; `shared/config/` without `index.ts`; `features/a/@x/b.ts`; `src/router.tsx` in `nextjs`; `nextjs` without a root `pages/README.md`; code in the root `pages/` |
| `file-names` | kebab names; `[invoiceId]`, `[[...rest]]`, `(.)photo` and `(group)` in the root `app/` of Next; `$invoiceId.tsx`, `.lazy.tsx`, `__root.tsx` and `route-tree.gen.ts` in Vite; `index.server.ts` in Next; `@x` in an entity | `UserCard.tsx`; `userCard.ts`; `user.helper.ts`; `[id]` in a Vite route folder; `$invoice_id`; `index.server.ts` in Vite; `@x` in a feature |
| `typescript-sources` | `.ts`, `.tsx`, `.d.ts`; `eslint.config.mjs` at the root | `src/shared/lib/format.js` |
| `import-direction` | widgets importing features, entities, shared; `src/app` importing all; the root `app/` importing `@/pages/home`; entity `invoice` importing `@/entities/user/@x/invoice` | entities importing features; feature `a` importing feature `b`; shared importing entities; entity `order` importing `@/entities/user/@x/invoice`; type-only imports count too |
| `import-paths` | `@/entities/user` from a feature; `./ui/card` inside a slice; `../lib/format` from `shared/ui`; `@/shared/ui/button`; `@/shared/config`; `@/features/auth/sign-in` in a group; `@/entities/user/index.server` in Next | `@icons/x`; `../../entities/user` from a feature; `@/features/auth/ui/form` from inside `features/auth`; `@/entities/user/model/store` from a widget, also as `export ... from` and `import()`; `@/shared/config/env`; `@/shared/ui/button/button` |
| `component-names` | `user-card.tsx` exporting `UserCard` and `UserCardHeader`; a Next `page.tsx` with a default export | `user-card.tsx` exporting `Card`; exporting `Avatar` too; `export default UserCard` in a `ui` segment |
| `hook-names` | `model/use-balance.ts` exporting `useBalance`; `shared/lib/use-media-query.ts`; a hook in `src/app/providers/` | `useBalance` exported from `model/balance.ts`; `use-balance.ts` exporting `getBalance`; a hook in `ui/` |
| `state-placement` | zustand `create` in `entities/cart/model/use-cart-store.ts`; `configureStore` in `src/app/store/store.ts` | `create` from zustand in `ui/`; a store in `shared/lib`; a store in `src/app/providers/`; an aliased import (`import { create as make }`) is still found |
| `api-boundary` | `fetch` in `shared/api/client.ts`; `entities/user/api/user-api.ts` importing `@/shared/api` | `fetch` in a component; `window.fetch` and `globalThis.fetch`; `axios` default and named imports in a hook; `@/shared/api` imported from a `ui` segment; a local function named `fetch` is not reported |
| `env-access` | literal reads in `env.ts` per pack; Next `server-env.ts` starting with `import "server-only"` | a read in a component; `process.env[key]`; `const { A } = import.meta.env`; `VITE_` key missing its prefix in Vite; a non-public key in Next `env.ts`; `server-env.ts` without the guard; any read in a library |
| `literal-text` | `{t("title")}`; numbers and punctuation; `className` strings | `<h1>Welcome back</h1>`; `placeholder="Search"`; text inside a fragment |
| `messages` | `en.json` and `de.json` with the same nested keys; `t("a.b")` present in `en` | a key only in `de`; an empty string; `t("a.c")` absent from `en`, bound per pack (`useTranslations("a")` then `t("c")` in Next) |
| `asset-placement` | `src/shared/ui/logo.svg`; `src/features/auth/ui/logo.png`; `public/favicon.ico`; Next root `app/icon.png` | `src/shared/assets/logo.svg`; `src/assets/hero.jpg` in an app; `src/entities/user/model/avatar.png` |
| `style-files` | `src/app/styles/globals.css` imported from the root `app/layout.tsx`; `user-card.module.css` beside `user-card.tsx` | a second global; `src/app/globals.css`; `card.module.css` with no `card.tsx`; `import "./card.css"` in a widget |
| `route-files` | the special files of each router; a thin `app/invoices/[invoiceId]/page.tsx` re-exporting `@/pages/invoice` | `app/(dashboard)/table.tsx` in Next; anything under `src/app/` checked as Next routes; a hook in `src/app/routes/`; a TanStack route file without `Route` |
| `client-boundary` | `"use client"` in `ui/` and in `src/app/providers/` | `"use client"` in the root `app/page.tsx` and `app/layout.tsx`; in `shared/lib/format.ts`; nothing is reported for `react-vite` |
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
4. The design revised for decision 11, one docs commit, read by the lead.
5. One code fix round: the 17 rules to decision 11, and the toolchain rules of decision 10
   (`import-alias`, `route-config`, `packages`) with their pages and both presets. Then the lane.
6. `docs/architecture.md` section 13 and `docs/guide/plugins.md` name the plugin; `CHANGELOG.md`
   (0.3.0, Added).

## Decided

The lead approved the design at `6ebd2b0` for code in v0.3.0 and answered the open questions:

1. **Kebab-case files**: yes, every file and folder under `src/` (2.2).
2. **Translation in every app**: yes (2.6); `literal-text` and `messages` run in every app.
3. **A `pages` layer in every app pack**, sliced like `widgets`; route files are thin and compose
   from `@/pages/<slice>` (2.1). This replaces the first answer, which had no `pages` layer
   (decision 11).
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
11. **The second independent FSD pass** (2026-11-20; the FSD documentation, Steiger 0.8.2 and
    fsd-lint 1.2.1) confirmed the first, and the lead ruled:
    - Next.js keeps the App Router in the root `app/` with thin route files, FSD in `src/` with the
      layers `src/app` and `src/pages`, and a required root `pages/README.md`; `layout` accepts the
      root `app/`, `route-files` reads it and not `src/app`; no `_app` or `_pages` rename (2.1, 2.7).
    - The `pages` layer exists in every app pack (decision 3).
    - `@x` on entities only, as 2.3 says; every rule that reads paths or imports accepts exactly that.
    - No `shared/assets`: imported assets sit in `ui` segments; the shared segments are `api`, `ui`,
      `lib`, `config`, `i18n` and `routes` (2.1, 2.6).
    - `shared` and `app` are one slice each; shared's public API is `@/shared/<segment>`, and for
      `ui` and `lib` one level down; every other shared segment has an `index.ts` (2.3).
    - A slice's public API is `index.ts`, and `index.server.ts` in Next.js; `index.tsx` is reported
      (2.1, 2.7).
    - Slice groups: one level, in `widgets`, `features` and `entities` (2.1).
    - The closed segment list and slice root stay, recorded as stricter than FSD (2.7).
    - The plugin checks slice public APIs itself (2.3).
    - `layout` adds: no `ui` in the app layer, Steiger's reserved folder names, no segment directly
      on a sliced layer. Not added: Steiger's heuristic rules, wildcard re-exports, circular
      imports (2.7).
    - The design writes down what the code already did: `instrumentation-client.ts`, `.lazy` files
      and `route-tree.gen.ts` in Vite, `[[...rest]]` and intercepting routes in Next.js, camelCase
      route parameters (2.2, section 3).

## Done when

- The lead decides on the design and the owner on the open questions. Then the code, with one lane
  round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the release build
  exit 0, and the leak scan prints nothing.
- Do not edit `docs/plan.md` unless the lead asks.

## Resume point

2026-11-20T02:02:49+00:00: the design is revised for the second FSD pass (decision 11) as one docs commit, for the
lead to read. The 17 rules are committed and their first lane round (ticket 283) is queued; they still
implement the earlier tree. Next, once the lead has read the design: one code fix round over the 17
rules and the three held rules of decision 10, then the lane.

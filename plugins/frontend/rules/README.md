# The `frontend` plugin

One standard for React apps and libraries. Someone who knows one frontend can find their way in
another: the same folders for the same jobs, the same names for the same kinds of file, imports that
point one way, one module that talks to the API, one module that reads the environment, every
user-visible string through the translation layer, assets and styles in known places, and one tool
for each job.

```toml
[plugins.frontend]
source = "ElsiKora/Gate:frontend@0.3.0"
options = { pack = "nextjs" }

[project]
extends = ["frontend/recommended"]
```

## The pack

`pack` is the plugin's one option, and every rule needs it. It names the framework:

| Pack | For |
| --- | --- |
| `react-vite` | a React app built with Vite and routed by TanStack Router file routes in `src/app/routes/` |
| `nextjs` | a Next.js app on the App Router, routed from the root `app/` |
| `react-library` | a React library: one shared layer published on its own, entry `src/index.ts` |

A missing or unknown pack is a configuration error, and the error lists the three packs. The plugin
never infers the pack. Everything in the standard holds for every pack, except where a framework forces
a difference: the route folder and its syntax, the files allowed directly in `src/`, the public API
files, how the environment is read, and the router and translation library. Each rule's page says which packs it runs for.

## The tree

The tree is Feature-Sliced Design, with closed lists where the methodology leaves a choice open:

```
src/
  app/           the application, one slice: entrypoint, providers, store, styles, config, lib,
                 and routes in a Vite app
  pages/         screens, one slice each
  widgets/       blocks of a page, one slice each
  features/      user actions, one slice each
  entities/      business objects, one slice each
  shared/        one slice of segments: api, ui, lib, config, i18n, routes
```

A slice is a kebab-case folder with an `index.ts`, its public API (and `index.server.ts` in
Next.js), and the segments `ui`, `model`, `api`, `lib` and `config`. `widgets`, `features` and
`entities` may group slices one level deep (`features/auth/sign-in/`). An entity may offer another
entity a cross-import file, `entities/user/@x/order.ts`. A segment of `shared` is reached through its
`index.ts`, except `ui` and `lib`, which are reached one level down (`@/shared/ui/button`). The app
layer has no `ui` segment, and assets sit in the `ui` segment of what shows them. Generated code has
one place, `src/shared/api/generated/` (a library's `src/api/generated/`): its names and its tree are
the generator's, and only the import rules read it.

A Next.js app keeps the App Router in the root `app/`, whose route files render pages of
`src/pages/`, and a root `pages/README.md` that keeps Next.js from reading `src/pages/` as the Pages
Router. A library's `src/` holds the segments `api`, `config`, `i18n`, `lib`, `model` and `ui`, and
`index.ts`.

## Presets

Set `pack` first: no preset can choose the framework, so a project that extends either preset writes
`[plugins.frontend] options pack`, and without it the check stops with the configuration error
above.

- `frontend/recommended`: every rule at `error`. For a project that meets the standard.
- `frontend/adoption`: every rule at `warning`. For an existing project moving to the standard, so it
  sees every departure without a failing gate. A Vite app still on React Router, or a Next.js app with
  client route files, starts here.

## What it does not cover

- **Nuxt and Vue.** No parser in Gate reads `.vue` files, so a Nuxt app is outside this plugin, and
  0.3.0 has no Vue pack.
- **The generic toolchain.** ESLint, Prettier, `next.config.*` and the rest of `package.json` and
  `tsconfig.json` belong to the `node` plugin. This plugin reads only what the standard needs of them:
  the alias `@/` (`frontend/import-alias`), the route folders of TanStack Router
  (`frontend/route-config`) and one tool per job (`frontend/packages`).
- **Tracked secrets and `.env` files** belong to the hygiene rules.

## Rules

| Rule | Holds |
| --- | --- |
| [`frontend/layout`](layout.md) | every file sits in the tree of the standard |
| [`frontend/file-names`](file-names.md) | names are kebab-case; route folders take their router's syntax |
| [`frontend/typescript-sources`](typescript-sources.md) | sources under `src/` are TypeScript |
| [`frontend/import-direction`](import-direction.md) | layers import only layers below them |
| [`frontend/import-paths`](import-paths.md) | one alias, `@/`, through the public API; relative imports stay inside a slice |
| [`frontend/component-names`](component-names.md) | a component file exports the component its name gives |
| [`frontend/hook-names`](hook-names.md) | a hook lives in a file of its own name, in `model` or `lib` |
| [`frontend/state-placement`](state-placement.md) | stores are created in `model` segments or `app/store` |
| [`frontend/api-boundary`](api-boundary.md) | only the shared `api` segment talks to the network |
| [`frontend/env-access`](env-access.md) | one module reads the environment, by literal public keys |
| [`frontend/literal-text`](literal-text.md) | user-visible text goes through the translation layer |
| [`frontend/messages`](messages.md) | locales match, and the code uses only messages that exist |
| [`frontend/asset-placement`](asset-placement.md) | imported assets sit in the `ui` segment of what shows them |
| [`frontend/style-files`](style-files.md) | one global stylesheet place; CSS modules beside their component |
| [`frontend/route-files`](route-files.md) | the route folder holds route files with their required exports |
| [`frontend/client-boundary`](client-boundary.md) | Next.js route files stay server components |
| [`frontend/colocated-files`](colocated-files.md) | tests and stories sit beside the file they are about |
| [`frontend/import-alias`](import-alias.md) | `@/` for `src/` is declared in the tsconfig and the Vite configuration, and no other alias |
| [`frontend/route-config`](route-config.md) | a Vite app runs TanStack Router's plugin on the standard's route folders |
| [`frontend/packages`](packages.md) | one tool per job, and the router and translation library of the pack |

Every finding names its target as the path, or as `<path>#<name>` when it is about one name in a file
(an import specifier, an export, a key), and an exception names that target.

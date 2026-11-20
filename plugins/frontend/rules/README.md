# The `frontend` plugin

One standard for React apps and libraries. Someone who knows one frontend can find their way in
another: the same folders for the same jobs, the same names for the same kinds of file, imports that
point one way, one module that talks to the API, one module that reads the environment, every
user-visible string through the translation layer, and assets and styles in known places.

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
| `nextjs` | a Next.js app on the App Router, routed from `src/app/` |
| `react-library` | a React library: one shared layer published on its own, entry `src/index.ts` |

A missing or unknown pack is a configuration error, and the error lists the three packs. The plugin
never infers the pack. Everything in the standard holds for every pack, except where a framework forces
a difference: the route folder and its syntax, the files allowed directly in `src/`, how the environment
is read, and the translation library. Each rule's page says which packs it runs for.

## The tree

```
src/
  app/           the application: entry, providers, global styles, the routes
  widgets/       blocks of a page, one slice each
  features/      user actions, one slice each
  entities/      business objects, one slice each
  shared/        segments only: api, config, i18n, assets, ui, lib
```

A slice is a kebab-case folder with an `index.ts` and the segments `ui`, `model`, `api`, `lib` and
`config`. A library's `src/` holds the shared segments (plus `model`) and `index.ts`. There is no
`pages` layer: in both app packs, the route file is the page.

## Presets

- `frontend/recommended`: every rule at `error`. For a project that meets the standard.
- `frontend/adoption`: every rule at `warning`. For an existing project moving to the standard, so it
  sees every departure without a failing gate. A Vite app still on React Router, or a Next.js app with
  client route files, starts here.

## What it does not cover

- **Nuxt and Vue.** No parser in Gate reads `.vue` files, so a Nuxt app is outside this plugin, and
  0.3.0 has no Vue pack.
- **The toolchain.** `package.json`, `tsconfig.json`, `vite.config.*`, `next.config.*`, ESLint and
  Prettier belong to the `node` plugin. The standard needs the tsconfig `paths` entry
  `"@/*": ["./src/*"]` (and the same alias in `vite.config.*`). In a Vite app it also needs TanStack
  Router's `routesDirectory` set to `src/app/routes` and its `generatedRouteTree` set to
  `src/app/route-tree.gen.ts`.
- **Public API imports.** `typescript/barrel-imports` reports an import that bypasses a slice's
  `index.ts`; enable it beside this plugin.
- **Tracked secrets and `.env` files** belong to the hygiene rules.

## Rules

| Rule | Holds |
| --- | --- |
| [`frontend/layout`](layout.md) | every file sits in the tree of the standard |
| [`frontend/file-names`](file-names.md) | names are kebab-case; route folders take their router's syntax |
| [`frontend/typescript-sources`](typescript-sources.md) | sources under `src/` are TypeScript |
| [`frontend/import-direction`](import-direction.md) | layers import only layers below them |
| [`frontend/import-paths`](import-paths.md) | one alias, `@/`; relative imports stay inside a slice or segment |
| [`frontend/component-names`](component-names.md) | a component file exports the component its name gives |
| [`frontend/hook-names`](hook-names.md) | a hook lives in a file of its own name, in `model` or `lib` |
| [`frontend/state-placement`](state-placement.md) | stores are created in `model` segments or the app layer |
| [`frontend/api-boundary`](api-boundary.md) | only the shared `api` segment talks to the network |
| [`frontend/env-access`](env-access.md) | one module reads the environment, by literal public keys |
| [`frontend/literal-text`](literal-text.md) | user-visible text goes through the translation layer |
| [`frontend/messages`](messages.md) | locales match, and the code uses only messages that exist |
| [`frontend/asset-placement`](asset-placement.md) | imported assets sit in the assets segment |
| [`frontend/style-files`](style-files.md) | one global stylesheet place; CSS modules beside their component |
| [`frontend/route-files`](route-files.md) | the route folder holds route files with their required exports |
| [`frontend/client-boundary`](client-boundary.md) | Next.js route files stay server components |
| [`frontend/colocated-files`](colocated-files.md) | tests and stories sit beside the file they are about |

Every finding names its target as the path, or as `<path>#<name>` when it is about one name in a file
(an import specifier, an export, a key), and an exception names that target.

# `frontend/route-config`

A Vite app runs TanStack Router's plugin on src/app/routes and src/app/route-tree.gen.ts.

## What it checks and why

`frontend/route-files` and `frontend/layout` hold the route folder of a Vite app at `src/app/routes/`
and its generated tree at `src/app/route-tree.gen.ts`; the router must read and write the same
places. In the Vite configuration (the first of `vite.config.js`, `.mjs`, `.ts`, `.cjs`, `.mts`, `.cts`,
in Vite's order):

- `plugin`: no Vite configuration, or one that does not call `tanstackRouter` (or
  `TanStackRouterVite`) of `@tanstack/router-plugin/vite`.
- `routes-directory`: the plugin's `routesDirectory` is missing or is not `./src/app/routes`.
- `route-tree`: the plugin's `generatedRouteTree` is missing or is not `./src/app/route-tree.gen.ts`.

A leading `./` is optional. A missing option is a finding, since the plugin's defaults are other
folders. Target `<config>#<option>`.

Packs: `react-vite`; the other packs report nothing.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `plugin`, `routes-directory`, `route-tree`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/route-config vite.config.ts
  vite.config.ts sets routesDirectory to ./src/routes, not ./src/app/routes
```

## How to fix it

Add `tanstackRouter({ target: "react", routesDirectory: "./src/app/routes", generatedRouteTree:
"./src/app/route-tree.gen.ts" })` of `@tanstack/router-plugin/vite` to the plugins of the Vite
configuration.

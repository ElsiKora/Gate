# `frontend/import-alias`

The alias @/ for src/ is declared in the tsconfig and the Vite configuration, and no other.

## What it checks and why

`frontend/import-paths` reads `@/` as `src/`; the type checker and the bundler must read it the same
way. The rule reads `tsconfig.json` as TypeScript does (comments and trailing commas allowed),
following a relative `extends` to the file that declares `compilerOptions.paths`:

- `tsconfig`: no `compilerOptions.paths`; no `@/*`; `@/*` mapped to anything but `["./src/*"]` (or
  `["src/*"]`); any other key of `paths`. Target `tsconfig.json#<key>`.
- `vite`: a Vite configuration (`vite.config.ts`, `.mts`, `.js` or `.mjs`) that neither maps `@` to
  `src` in `resolve.alias` (object form, or `{ find: "@", replacement }` in the array form) nor calls
  the `vite-tsconfig-paths` plugin. A project without a Vite configuration is not read for `vite`.

A file that is not JSON stops the run with an error that names it.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `tsconfig`, `vite`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/import-alias tsconfig.json
  tsconfig.json declares the alias ~/*; the one alias is @/*
```

## How to fix it

Declare `"paths": { "@/*": ["./src/*"] }` in `compilerOptions` and nothing else, and in the Vite
configuration either `resolve: { alias: { "@": path.resolve(__dirname, "./src") } }` or the
`vite-tsconfig-paths` plugin.

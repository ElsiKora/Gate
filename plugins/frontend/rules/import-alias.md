# `frontend/import-alias`

The alias @/ for src/ is declared in the tsconfig and the Vite configuration, and no other.

## What it checks and why

`frontend/import-paths` reads `@/` as `src/`; the type checker and the bundler must read it the same
way. The rule reads the tsconfig `[languages.typescript]` names (`tsconfig.json` by default) as
TypeScript does (comments and trailing commas allowed): through its `references` (create-vite's
`files: []` beside `tsconfig.app.json`) and, in each, through its relative `extends` (a string or an
array, the last entry winning, a path with or without `.json`) to the file that declares
`compilerOptions.paths`:

- `tsconfig`: no config on the way declares `compilerOptions.paths`; in a config that does, no `@/*`,
  `@/*` mapped to anything but `["./src/*"]` (or `["src/*"]`), or any other key of `paths`. Target
  `<config>#<key>`.
- `vite`: a Vite configuration (the first of `vite.config.js`, `.mjs`, `.ts`, `.cjs`, `.mts`, `.cts`,
  in Vite's order) that neither maps `@` to `src` in `resolve.alias` (object form, or
  `{ find, replacement }` in the array form with `find` `"@"` or `/^@\//`) nor calls the
  `vite-tsconfig-paths` plugin. An alias whose value the rule cannot read without running the file (a
  variable, a spread) gives no finding. A project without a Vite configuration is not read for `vite`.

A library imports by relative paths and is not read.

A file that is not JSON stops the run with an error that names it.

Packs: `react-vite` and `nextjs`; a library reports nothing.

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

# `frontend/typescript-sources`

Sources under src/ are TypeScript.

## What it checks and why

The other rules read TypeScript; a JavaScript file is unchecked code. A `.js`, `.jsx`, `.mjs` or
`.cjs` file under `src/` is reported. Configuration files at the project root (`eslint.config.mjs`)
are not under `src/` and are not read.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/typescript-sources src/shared/lib/format.js
  src/shared/lib/format.js is JavaScript; sources under src/ are TypeScript
```

## How to fix it

Rename the file to `.ts` or `.tsx` and type it.

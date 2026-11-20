# `frontend/hook-names`

A hook lives in a file of its own name, in a model or lib segment.

## What it checks and why

Hooks are found by name and live where state and helpers live. A hook is an exported value named
`use` and a capital letter; re-exports (`export { useX } from "…"`) and `index` files are not read,
nor are files with a dotted part.

- `file`: a hook exported from a file not named after it: `useBalance` belongs in
  `use-balance.ts` (or `.tsx`). Target `<path>#<hook>`.
- `export`: a `use-*` file that does not export the hook its name gives.
- `placement`: a `use-*` file, or a file that exports a hook, outside a `model` or `lib` segment and
  the app layer's `providers`, `store` and `lib`. A store built by a store library is a hook too
  (`use-cart-store.ts`).

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `file`, `export`, `placement`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/hook-names src/entities/balance/ui/use-toggle.ts
  src/entities/balance/ui/use-toggle.ts holds a hook outside a model or lib segment and the app layer's providers, store and lib
```

## How to fix it

Move the hook into a file of its own name in the `model` segment of its slice, or into a `lib` segment when it knows no business.

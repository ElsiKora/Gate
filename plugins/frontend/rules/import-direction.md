# `frontend/import-direction`

Layers import only layers below them, and slices no slice of their own layer.

## What it checks and why

Layers that point one way keep a slice replaceable: `app` > `pages` > `widgets` > `features` >
`entities` > `shared`. For every import, re-export, `import()` and `import("…")` type of a `.ts` or
`.tsx` file in a layer (type-only imports included), the specifier is resolved with the tsconfig
`paths`; an import of a package, or one that does not resolve to a tracked file, is not read. The
root `app/` of a Next.js app is above every layer and is not read.

- `layer`: the target is in a higher layer than the importer. `shared` therefore imports no other
  layer.
- `cross-slice`: the target is another slice of the importer's own layer. One entity may import
  another through the `@x` file the other keeps for it: `src/entities/user/@x/order.ts` is open to
  the entity `order`, and to no other slice.

The app and shared layers are one slice each. Files directly in `src/` are not read. Target
`<path>#<specifier>`; one finding per import.

Packs: `react-vite` and `nextjs`; a library has no layers and is not read.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `layer`, `cross-slice`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/import-direction src/entities/user/index.ts:2
  src/entities/user/index.ts imports @/features/sign-in, a module of the higher layer features; entities imports only layers below it
```

## How to fix it

Move what both need down to the lower layer, or compose the two in the higher one. Two slices of
one layer are composed by a slice of a higher layer or by the app; two entities that must know each
other meet through an `@x` file.

# `frontend/state-placement`

Stores are created in a model segment or the app's store.

## What it checks and why

State has one home per slice, and the root store is the app's. A call of a store constructor (an
export of a package that `stores` lists) is reported outside a `model` segment and `src/app/store/`.
The callee is followed through the file's imports as the TypeScript checker follows names: named and
renamed imports, namespace imports, `const` aliases and destructuring; a local function of the same
name is not a store. Target `<path>#<export>`.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Findings
name their path as the target unless the text above names another.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `stores` | list of `{ package, export }` | `zustand` `create`, `@reduxjs/toolkit` `createSlice` and `configureStore`, `recoil` `atom`, `jotai` `atom`, `mobx` `makeAutoObservable` | The functions that create a store. The package is matched exactly. |

Every rule also reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/state-placement src/entities/cart/ui/cart.tsx:2
  src/entities/cart/ui/cart.tsx creates a store with create of zustand outside a model segment and src/app/store/
```

## How to fix it

Create the store in the `model` segment of its slice, or the root store in `src/app/store/`.

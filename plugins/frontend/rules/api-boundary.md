# `frontend/api-boundary`

Only the API segment of the shared layer talks to the network.

## What it checks and why

One client holds the base URL, the authorization header, retries and error mapping; a raw call skips
all of them. Outside `src/shared/api/` (a library's `src/api/`), in the files of `src/`, of Next.js's
root `app/` and Next.js's root files (`middleware.ts`, `proxy.ts`, `instrumentation.ts`,
`instrumentation-client.ts`):

- `raw-call`: an import (not type-only) of an HTTP package of `packages`, or of a subpath of it; a
  reference to a global of `globals` that the file does not declare, also as a member of `window`,
  `globalThis` or `self` (`window.fetch`). Target `<path>#<package or global>`.
- `client-import`: an import that resolves into `src/shared/api/` from a file outside an `api`
  segment and the app layer (and a library's `src/index.ts`, which re-exports it). Endpoints live in
  the `api` segments of slices; components and hooks call those. Target `<path>#<specifier>`.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `raw-call`, `client-import`. Findings
name their path as the target unless the text above names another.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `packages` | list of strings | `axios`, `ky`, `ofetch`, `cross-fetch`, `node-fetch`, `isomorphic-fetch`, `undici`, `graphql-request` | The HTTP packages. |
| `globals` | list of strings | `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource` | The globals that reach the network. |

Every rule also reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/api-boundary src/entities/user/ui/user-card.tsx:2
  src/entities/user/ui/user-card.tsx calls the network through fetch outside src/shared/api/
```

## How to fix it

Call the endpoint through the `api` segment of its slice, which uses the client of `src/shared/api/`.

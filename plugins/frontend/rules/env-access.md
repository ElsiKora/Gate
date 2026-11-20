# `frontend/env-access`

One module reads the environment, each key by a literal member with the public prefix.

## What it checks and why

Bundlers replace only literal reads of public keys, and one module makes the list of every key the
app needs. The environment modules are `src/shared/config/env.ts`, and in Next.js also
`src/shared/config/server-env.ts` for keys the browser must never see; an inventory of the app's
environment keys reads them. The files of `src/`, of Next.js's root `app/` and Next.js's root files
(`middleware.ts`, `proxy.ts`, `instrumentation.ts`, `instrumentation-client.ts`) are read. A read is
`import.meta.env` or `process.env` (when the file declares no `process`):

- `outside`: a read in any other file; in a library, any read at all.
- `dynamic`: in an environment module, a read other than a literal member: `env[key]`, destructuring,
  spreading, passing `env` along.
- `prefix`: in `env.ts`, a key without the pack's public prefix: `VITE_` (or Vite's own `MODE`,
  `DEV`, `PROD`, `SSR`, `BASE_URL`) for `react-vite`, `NEXT_PUBLIC_` (or `NODE_ENV`) for `nextjs`; and
  a read in the other pack's syntax (`process.env` in a Vite app, `import.meta.env` in Next.js).
- `server-guard`: in Next.js, a `server-env.ts` whose first statement is not `import "server-only"`.

Target `<path>#<key>` for a key without its prefix, `<path>#import.meta.env` or `<path>#process.env`
for the other reads.

Packs: all three; `server-guard` and `server-env.ts` for `nextjs`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `outside`, `dynamic`, `prefix`, `server-guard`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/env-access src/shared/config/env.ts:2
  src/shared/config/env.ts reads DATABASE_URL, which has no NEXT_PUBLIC_ prefix
```

## How to fix it

Read each key once, as `import.meta.env.VITE_<KEY>` or `process.env.NEXT_PUBLIC_<KEY>`, in `env.ts`,
export it typed, and import the value elsewhere. In Next.js, read server keys in `server-env.ts`,
which starts with `import "server-only"`. A library takes its values from its caller.

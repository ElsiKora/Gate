# `frontend/client-boundary`

Route files stay server components; "use client" marks ui and model modules.

## What it checks and why

In Next.js, a route file that is a client component loses server data loading and its `metadata`.
A file whose first statements carry the directive `"use client"`:

- `route-file`: is a route file (a special file of `src/app/`, as `frontend/route-files` reads it).
- `placement`: is outside a `ui` or `model` segment and outside `src/app/` itself (the app's
  providers). Helpers and hooks need no directive; the boundary is a component.

Packs: `nextjs`; the other packs have no server components and report nothing.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `route-file`, `placement`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/client-boundary src/app/page.tsx
  src/app/page.tsx is a route file marked "use client"; route files stay server components
```

## How to fix it

Move the interactive part into a component of a `ui` segment marked `"use client"` and render it from the server route file.

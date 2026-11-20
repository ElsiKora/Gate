# `frontend/route-files`

The route folder holds route files only, each with the export its router requires.

## What it checks and why

A route folder that holds only routes reads as the app's map; components, hooks and helpers belong
to layers. The route file reads its parameters and renders a page of `src/pages/`.

- `react-vite`, `src/app/routes/`: `foreign` is a file that is not `.ts` or `.tsx` or carries a dotted
  part other than `lazy`; `export` is a route file without the export `Route` (target
  `<path>#Route`).
- `nextjs`, the root `app/`: a `.ts` or `.tsx` file is a route file when it is a special file of a
  segment (`page`, `layout`, `loading`, `error`, `not-found`, `template`, `default`, `global-error`,
  `global-not-found`, `forbidden`, `unauthorized`, the metadata files `sitemap`, `robots`, `manifest`, `icon`,
  `apple-icon`, `opengraph-image`, `twitter-image`, and `route`). `foreign` is any other code file in
  `app/`. `export` is a special file without a default export, or a `route` file that exports no
  HTTP method handler (`GET`, `HEAD`, `POST`, `PUT`, `DELETE`, `PATCH`, `OPTIONS`).

A file with an `export *` may export what is required and is not reported for `export`. Stylesheets
and images in the route folder are read by `frontend/style-files` and `frontend/asset-placement`.

Packs: `react-vite` and `nextjs`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `foreign`, `export`. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/route-files app/(dashboard)/table.tsx
  app/(dashboard)/table.tsx is not a Next.js route file; app/ holds route files only
```

## How to fix it

Move the component, hook or helper into the layer and slice of its job and import it from the route file; give a route file the export its router requires.

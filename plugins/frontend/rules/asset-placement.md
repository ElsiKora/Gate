# `frontend/asset-placement`

Assets the code imports sit in the ui segment of what uses them.

## What it checks and why

An asset moves with the component that shows it, and files served by a fixed URL (favicon,
`robots.txt`, manifest, social images) go to `public/`, which the rule does not read. A file under
`src/` with an extension of `extensions` (compared without case) is reported outside a `ui` segment:
a slice's, `src/shared/ui/`, or a library's `src/ui/`. The app layer has no `ui` segment. In Next.js,
the root `app/` holds only the metadata images Next.js reads from the route folder: `icon`,
`apple-icon`, `opengraph-image` and `twitter-image` (with an optional number) anywhere in it,
`favicon` in `app/` itself.

Packs: all three; the metadata images for `nextjs`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Findings
name their path as the target unless the text above names another.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `extensions` | list of strings | `png jpg jpeg gif webp avif svg ico woff woff2 ttf otf mp3 wav mp4 webm` | The extensions of asset files, without the dot. |

Every rule also reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/asset-placement src/features/auth/model/logo.png
  asset src/features/auth/model/logo.png sits outside a ui segment and public/
```

## How to fix it

Move the file into the `ui` segment of the slice that shows it, or of `src/shared/ui/` when several do; move it to `public/` when it is served by a fixed URL.

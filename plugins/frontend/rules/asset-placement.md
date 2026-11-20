# `frontend/asset-placement`

Assets the code imports sit in the assets segment.

## What it checks and why

Assets have two homes: files the code imports go to `src/shared/assets/` (a library's
`src/assets/`), where the bundler hashes them; files served by a fixed URL (favicon, `robots.txt`,
manifest, social images) go to `public/`. A file under `src/` with an extension of `extensions`
(compared without case) is reported anywhere else. In Next.js, the metadata images Next.js reads from
the route folder are allowed in `src/app/`: `icon`, `apple-icon`, `opengraph-image` and
`twitter-image` (with an optional number) anywhere in it, `favicon` in `src/app/` itself.

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
error frontend/asset-placement src/features/auth/ui/logo.png
  asset src/features/auth/ui/logo.png sits outside src/shared/assets/
```

## How to fix it

Move the file to `src/shared/assets/` when the code imports it, or to `public/` when it is served by a fixed URL.

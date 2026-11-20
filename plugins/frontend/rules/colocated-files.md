# `frontend/colocated-files`

Tests and stories sit beside the file they are about.

## What it checks and why

Tests and stories move with their code. A file under `src/` with the dotted part `test` or
`stories` (`card.test.tsx`, `card.stories.tsx`) is reported when its folder holds no other file of
the same name before the first dot that is neither a test nor a story (`card.tsx`).

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Findings
name their path as the target unless the text above names another.

## Options

None. The rule reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/colocated-files src/shared/ui/card/badge.stories.tsx
  src/shared/ui/card/badge.stories.tsx has no badge file beside it
```

## How to fix it

Keep the test or story beside the file it is about, with its name; delete it when that file is gone.

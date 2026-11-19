# `nestjs/repeated-segment`

A file name does not repeat a folder segment its path already represents.

## What it checks and why

A folder already names its area, so a file inside it does not say it again:
`shelf/utility/shelf-check.utility.ts` reads `shelf` twice, where `shelf/utility/check.utility.ts`
says the same once. Repeated words make names long and hide the part that tells files apart.

The rule judges every tracked `<stem>.<kind>.ts` file below `layout.source-root`, in every folder
(the walk skips none of `[languages.typescript] skip-folders`). Declaration files, files without a kind (`type.ts`) and stems in `ignored-stems` are left
out. A folder segment is repeated when its dash-separated words occur, whole and contiguous, among
the stem's words: `book-loan/` is repeated by `read-book-loan-state`, not by `book-loanless`.

The structural segments are never judged, because a name may legitimately start with them:

- the segments of `layout.source-root` itself;
- a folder of `layout.source-owners` directly below the source root (`src/config/`);
- a folder of `layout.module-planes` directly below `layout.modules-root` (`src/modules/domain/`).

The same names elsewhere in the path are judged. The first repeated segment is reported, once per
file, located at the file.

Scope: project. Language: `typescript`. Reads history: no. One aspect (`""`). Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `ignored-stems` | array of string | `["index"]` | File stems that are never judged. |

The rule reads `layout.source-root`, `layout.source-owners`, `layout.modules-root` and
`layout.module-planes` from `[plugins.nestjs] options`.

```toml
[rules]
"nestjs/repeated-segment" = { level = "error", options = { ignored-stems = ["index"] } }
```

## Example finding

```text
error nestjs/repeated-segment src/shelf/utility/shelf-check.utility.ts
  src/shelf/utility/shelf-check.utility.ts repeats parent folder segment "shelf" in its filename
```

## How to fix it

Drop the repeated words from the file name (`shelf-check.utility.ts` becomes `check.utility.ts`),
and update the imports of the file.

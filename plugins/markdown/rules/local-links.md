# `markdown/local-links`

Every local link destination of a documentation page names a tracked file or directory.

## What it checks and why

A link to a page that was moved or deleted still renders, and a reader finds the break only by
clicking it. The rule reads each page with a CommonMark parser (with GFM tables and strikethrough,
footnotes, math and wiki links) and resolves every destination of:

- an inline link or image: `[text](path)`, `![alt](path)`;
- an autolink: `<scheme:...>`;
- a link reference definition: `[label]: path`, each one, also when a label is defined twice.

A destination is local unless it is empty, starts with `/` or `#`, has a URI scheme (`https:`,
`mailto:`, any `letter(letter|digit|+|.|-)*:`), or contains `*`. An e-mail autolink
(`<someone@mail.example>`) is a `mailto:` link and is not read. Of a local destination the rule
keeps the part before `?` or `#`, decodes `%XX` escapes, and resolves it against the page's
directory. It exists when it is the project root, a tracked file, or a directory with a tracked file
below it. A destination that leaves the project (`../../outside.md` from `docs/a.md`) cannot be
seen, so it is reported.

The front matter block is not read, and neither is code: inline code, fenced or indented blocks,
HTML.

The pages are the tracked `.md` files below each of `roots` and every tracked file of `extra-files`.
A page that is not UTF-8 text stops the run with an error that names it.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `roots` | array of string | `["docs"]` | Directories, relative to the project, whose `.md` files are pages. `""` is the whole project. |
| `extra-files` | array of string | `[]` | Further pages, whatever their extension. A file that is not tracked is skipped. |

```toml
[rules]
"markdown/local-links" = { level = "error", options = { roots = ["docs", "handbook"], extra-files = ["deploy/README.md"] } }
```

## Example finding

`docs/guide/index.md`:

```md
Read [the setup](setup.md) first.
```

```text
error markdown/local-links docs/guide/index.md:1
  docs/guide/index.md links to missing file: setup.md
  help: point the link at a tracked file or directory, relative to the page; if the target moved,
  follow it, and if it is gone, remove the link
```

The finding's target is `<page>:<destination>`, as written: `docs/guide/index.md:setup.md`.

## How to fix it

Point the link at the file where it is now, relative to the page, or remove it. If the target is a
generated or untracked file, link to its source, or leave the page out of `roots`.

# `markdown/frontmatter`

Documentation pages open with front matter that holds each required key.

## What it checks and why

Site generators, indexes and search read a page's title, description and status from its front
matter. A page without them is missing from those lists or shows a file name. The rule reads the
block that opens the page with `---` and ends at the next line that is exactly `---`, and requires
each of `required-keys` on exactly one line of it, as `key:`, then spaces or tabs, then a value. A
key without a value, a key set twice, or a page without the block is reported, once per page.

- A page under one of `exclude` is not checked (pages with a front matter format of their own,
  such as agent skills).
- A page under one of `record-directories` (decision records, ledgers) keeps its own record format:
  it is checked only when it opens with `---`. A `README.md` there is always checked.
- A page that is not UTF-8 is read with replacement characters.

The pages are the tracked `.md` files below each of `roots` and every tracked file of `extra-files`.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `roots` | array of string | `["docs"]` | Directories, relative to the project, whose `.md` files are pages. `""` is the whole project. |
| `extra-files` | array of string | `[]` | Further pages, whatever their extension. A file that is not tracked is skipped. |
| `required-keys` | array of string | `["title", "description", "category", "status"]` | The keys each page must set. At least one; an empty list is a configuration error. |
| `exclude` | array of string | `["docs/agents/skills/"]` | Path prefixes of pages that are not checked. |
| `record-directories` | array of string | `["docs/ledger/", "docs/adr/"]` | Path prefixes whose pages are checked only once they open a front matter block, except `README.md`. |

```toml
[rules]
"markdown/frontmatter" = { level = "error", options = { required-keys = ["title", "owner"], record-directories = ["docs/decisions/"] } }
```

## Example finding

`docs/operations/deploy.md`:

```md
---
title: Deploy
---
# Deploy
```

```text
error markdown/frontmatter docs/operations/deploy.md
  docs/operations/deploy.md must have title, description, category, and status frontmatter
  help: open the page with a block between two `---` lines that sets title, description, category,
  and status, one `key: value` line each
```

The finding's target is the page's path.

## How to fix it

Open the page with the block and set every key on a line of its own:

```md
---
title: Deploy
description: How a release reaches production
category: operations
status: current
---
```

# `markdown/required-sections`

Documentation pages have the level-two sections their path requires.

## What it checks and why

A book of pages of one kind (one per module, per service, per runbook) is useful when each page has
the same parts in the same words. The `sections` option maps a path pattern to the `##` headings a
page must have. For each page, the first entry whose `path` matches the page's whole path decides;
each heading it names that the page does not have is one finding, in the entry's order.

- `path` is a regular expression in Rust `regex` syntax, matched against the whole path relative to
  the project (it is anchored as `^(?:path)$`). It must also be valid on its own; an invalid pattern
  is a configuration error that names it.
- A heading is a line of up to three spaces, `##`, at least one space or tab, and the text; closing
  `#` characters after white space and trailing white space are not part of the text. The text must
  equal the section exactly.
- Headings inside fenced code blocks and HTML comments do not count. Setext (underlined) headings
  are not read.
- No entry, or an entry with no sections, reports nothing.

The pages are the tracked `.md` files below each of `roots` and every tracked file of `extra-files`.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `roots` | array of string | `["docs"]` | Directories, relative to the project, whose `.md` files are pages. `""` is the whole project. |
| `extra-files` | array of string | `[]` | Further pages, whatever their extension. A file that is not tracked is skipped. |
| `sections` | array of `{ path, sections }` | `[]` | `path`: a regular expression for the whole page path; `sections`: the `##` heading texts the page must have. The first matching entry wins. |

```toml
[rules."markdown/required-sections"]
level = "error"

[rules."markdown/required-sections".options]
sections = [
  { path = 'docs/modules/README\.md', sections = ["Chapter Map"] },
  { path = 'docs/modules/[a-z-]+\.md', sections = ["Purpose", "Source Map"] },
]
```

## Example finding

`docs/modules/billing.md`:

```md
## Purpose

Charges and refunds.
```

```text
error markdown/required-sections docs/modules/billing.md
  docs/modules/billing.md is missing module documentation section: Source Map
  help: add a `## Source Map` heading with its content, outside code fences and HTML comments
```

The finding's target is `<page>:<section>`: `docs/modules/billing.md:Source Map`.

## How to fix it

Add the heading with its content. If the page is not of that kind, narrow the entry's `path`, or put
an entry for it before the broader one.

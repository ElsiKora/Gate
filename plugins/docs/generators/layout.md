# `docs/layout`

The required index pages of the project's documentation standard, as seeds.

## What it writes

For the kind in the `[plugins.docs]` options (see the rule [`docs/layout`](../rules/layout.md)), the
index of every required entry: `docs/README.md`, `docs/start-here.md`, and the `README.md` of
`architecture/`, `modules/`, `adr/`, `reference/`, `guide/`, `testing/` and `operations/`, and for
a library `releases/`. That is nine files for a service, a frontend and infrastructure, ten for a
library, and eight for a monorepo root, which has no `modules/`.

Each file is a seed: `gate sync` writes it only when nothing exists at its path (no file, link or
directory, tracked or not) and never compares it afterwards, so a page a person filled is never
overwritten. `gate sync --check` never reports a seed; a missing required page is a `missing`
finding of the rule instead, and `gate sync` is its fix. A required page deleted on purpose comes
back on the next `gate sync`.

Each seed holds:

- the page's title as its level-1 heading;
- the marker line `<!-- gate:seed: replace this page's text, then delete this line -->`, which the
  rule reports as `unfilled` until it is deleted;
- the kind's required sections as level-2 headings, in order, each with one line that says what goes
  there.

`docs/README.md` adds a table of the seeded entries, in the standard order, each linked, then a row
for each delegated entry listed in `delegated`, linking its directory. The decision log,
`docs/adr/README.md`, adds the record template as a fenced block.

The generator never writes an optional, delegated or declared entry. `agents/` and `ledger/` are
laid down by their own plugins.

## Options

None. It reads `kind` and `delegated` from the `[plugins.docs]` options; a missing or unknown kind
is a configuration error (exit 2).

```toml
[plugins]
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "frontend" } }

[rules]
"docs/layout" = "error"

[generate]
"docs/layout" = {}
```

## Example

`docs/architecture/README.md` of a frontend:

```md
# Architecture

<!-- gate:seed: replace this page's text, then delete this line -->

## System shape

The parts of the system and how they connect.

## Routing

The routes, and how a request reaches a page.

## State

Where state lives: server data, client state, the URL.

## Rendering

Where pages render (in the browser, on the server, or both), and why.

## API access

How the frontend calls its back ends, and how it handles their failures.
```

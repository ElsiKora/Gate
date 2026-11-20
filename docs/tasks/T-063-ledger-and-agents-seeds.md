# T-063: The ledger and agents plugins seed their own minimum

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead's decision; no code |
| Release | v0.3.0 |
| Writer | "Gate 005" |
| Reviewer | a separate review session |
| Depends on | T-062 (the seed mechanism, `Generate::seed`, generators that read the plugin options) and T-061 (the `input` aspect, the `agents-block` finding on a missing `AGENTS.md`); the code goes on a tip that carries both |
| Touches | `plugins/ledger` (a generator `ledger/layout`, its page), `plugins/agents` (`agents/agents-block`: a seed of the entry; its page), tests, `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

One `gate sync` lays down a project's whole documentation standard, the delegated entries included.
T-062 seeds the indexes of `docs/`; it leaves `docs/agents/` and `docs/ledger/` to their own
plugins, and T-061 makes those plugins' rules fail when their inputs are missing. This task makes
the two plugins seed the minimum their own rules need, through the same seed mechanism:

- the seed is written only where nothing exists, and never overwritten;
- `gate sync --check` never reports a seed;
- what is still missing after `gate sync` is a finding of the rule that reads it, as T-061 decided.

The decisions for the lead are in "Open", below.

## 1. The ledger: a generator `ledger/layout`

A new generator of the `ledger` plugin. It reads the `[plugins.ledger] options` (T-062 passes them
to a generator), so every path follows the project's configuration.

- **The manifest**, at `ledger/manifest`'s `manifest` option (default `docs/ledger/manifest.json`).
  This is the one file only the plugin can write correctly, because its shape is the plugin's own:
  `canonicalRoot` (the directory of the shared `decisions` path), `canonicalFiles` (the five shared
  paths below that root), `idFamilies: {}`, `forbiddenReferences: []`.
  `idFamilies` starts empty: `ledger/manifest#id-family` reports a family whose pattern matches
  nothing, so a seeded family on an empty ledger would fail at once. The project adds a family when
  it has records of it.
- **The five shared files**: `decisions`, `implementation`, `verification`, `follow-ups` and
  `blockers`, at their configured paths. Each is a title and the empty structure its rules read (the
  index's table header; the record files' titles). The ledger's rules accept an empty ledger, which
  is a complete state, so none of these seeds carries a marker line (see Open, 2).
- **A tracked manifest** names the ledger's own files. When one is tracked, the generator also seeds
  every `canonicalFiles` entry it lists that does not exist, with only a title. It reads that one
  file; it never parses a record.

No seed of `parity`'s plan, `catalog` or `coverage`: those are the project's own, and T-061 makes
their options required (`field`, `catalog`, `parity`) or reports them as `#input`.

The generator owns no glob. A seed under an owned glob is refused (T-062), and the ledger is the
project's from the moment it is written.

## 2. The agents plugin: `agents/agents-block` seeds the entry

T-061 makes `agents/agents-block` report a missing entry (`AGENTS.md`): "create it with the
`gate:begin agents` and `gate:end agents` lines". With seeds the generator can create it instead.

- When `entry` is not tracked, it seeds it: a title, one line on what the file is, and the managed
  block between its markers, filled as it would fill it. The T-061 finding for a missing entry goes
  away. Its finding for an empty `<root>/rules` stays.
- When `entry` is tracked, nothing changes: it writes the block between the markers, an owned write
  as today.
- An untracked `AGENTS.md` that exists is left alone by the core. The generator sees only tracked
  files, so it seeds, and the core then skips the seed because the path exists.

**The rule files** (`<root>/rules/*.md`) are not seeded (see Open, 1). A valid rule file needs an
`area` and an `owner` that names a decision of the project's ledger (`agents/rule-owner`), and no
generic seed has either. T-061's `#input` at `<root>/rules` stays the one finding. Its message says
what to write.

## 3. What one `gate sync` gives a service

With `docs/layout`, `ledger/layout` and `agents/agents-block` in `[generate]`, on an empty project:

| Plugin | Seeds | What `gate check` reports afterwards |
| --- | --- | --- |
| `docs` | the 9 indexes (T-062) | `docs/layout#unfilled` per index until its marker line is deleted; `docs/layout#index` for `docs/agents/` and `docs/ledger/`, now present and not in the seeded table (Open, 3) |
| `ledger` | `manifest.json` and the five shared files | nothing from the ledger rules |
| `agents` | `AGENTS.md` with its block | `#input` at `docs/agents/rules` from each rule-file rule (T-061) |

## 4. Tests

- `ledger/layout`, native:
  - on an empty project it seeds the manifest and the five files at the defaults, and at configured
    paths when the options move them;
  - the seeded ledger passes every ledger rule except those whose options T-061 makes required;
  - with a tracked manifest that lists a sixth canonical file, it seeds that file too;
  - every file is a seed.
- `agents/agents-block`, native:
  - with no `AGENTS.md` it returns one seed holding the markers and the block, and no finding;
  - with `AGENTS.md` tracked, its output is as today.
- End to end, once: an empty service with the three generators and the docs, ledger and agents
  rules on.
  - `gate sync` writes 9 + 6 + 1 files.
  - `gate check` gives exactly the findings in the table above.
  - A second `gate sync` writes nothing.
  - `gate sync --check` exits 0.

## Open, for the lead

1. **Rule files.** No seed (recommended): a rule file needs the project's own area and an owner
   decision, so a generic one would be reported by `agents/rule-owner` at once. The alternative
   seeds `docs/agents/rules/general.md` with a placeholder owner that the rule reports until it is
   set. That is a seed made to fail.
2. **Markers in ledger seeds.** None (recommended): an empty ledger is complete, and a marker would
   need every ledger rule to know about it. The alternative is a marker line, reported by
   `ledger/manifest` until it is deleted.
3. **The docs index and the delegated entries.** T-062's seeded `docs/README.md` lists only the
   entries `docs/layout` seeds. Once these seeds put `docs/agents/` and `docs/ledger/` in place,
   `docs/layout#index` asks for their rows.
   - (a) Recommended: no change. The finding names the row to add, once, in a page the person is
     filling anyway.
   - (b) The docs seed always lists `agents/` and `ledger/` for the kinds that have them. A project
     without those plugins then holds broken links, which `markdown/local-links` reports.
   - (c) `docs/layout#index` stops requiring rows for delegated entries. That weakens the index for
     every reader.

## Done when

- The lead decides on the three open items. Then the code goes on a tip that carries T-061 and T-062,
  followed by one lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`),
  deny and the release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Resume point

2026-11-20: design written on `task/T-063-seeds` from T-062's tip `ec96a38`, read against T-061's
branch (its `input` aspect, the `agents-block` finding, `ledger/manifest`'s fields and default path).
No code yet: T-061 sits on T-059's tip and T-062 on `main`, so the code waits for a tip that carries
both. Waiting for the lead's decision on the open items.

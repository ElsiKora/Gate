# T-063: The ledger and agents plugins seed their own minimum

| Field | Value |
| --- | --- |
| Status | code, on a working base that merges T-062 (`7fb79d5`) and T-061 (`6a286e2`); rebased when T-061 is green |
| Release | v0.3.0 |
| Writer | "Gate 005" |
| Reviewer | a separate review session |
| Depends on | T-062 (the seed mechanism, `Generate::seed`, generators that read the plugin options) and T-061 (the `input` aspect, the `agents-block` finding on a missing `AGENTS.md`); the code goes on a tip that carries both |
| Touches | `plugins/ledger` (a generator `ledger/layout`, its page, `Settings::checked` for a generator), `plugins/agents` (`agents/agents-block`: a seed of the entry; its page), tests, `docs/guide/plugins.md`, the changelogs |

## Goal

One `gate sync` lays down a project's whole documentation standard, the delegated entries included.
T-062 seeds the indexes of `docs/`; it leaves `docs/agents/` and `docs/ledger/` to their own
plugins, and T-061 makes those plugins' rules fail when their inputs are missing. This task makes
the two plugins seed the minimum their own rules need, through the same seed mechanism:

- the seed is written only where nothing exists, and never overwritten;
- `gate sync --check` never reports a seed;
- what is still missing after `gate sync` is a finding of the rule that reads it, as T-061 decided.

The lead's decisions are in "Decided", below.

## 1. The ledger: a generator `ledger/layout`

A new generator of the `ledger` plugin. It reads the `[plugins.ledger] options` (T-062 passes them
to a generator), so every path follows the project's configuration.

- **The manifest**, at the generator's own `manifest` option (default `docs/ledger/manifest.json`,
  the default of `ledger/manifest`'s option; a generator reads only its own options and the plugin's).
  This is the one file only the plugin can write correctly, because its shape is the plugin's own:
  `canonicalRoot` (the deepest directory that holds the five shared paths), `canonicalFiles` (the
  five shared paths below that root, a path two options name once), `idFamilies: {}`,
  `forbiddenReferences: []`.
  `idFamilies` starts empty: `ledger/manifest#id-family` reports a family whose pattern matches
  nothing, so a seeded family on an empty ledger would fail at once. The project adds a family when
  it has records of it.
- **The five shared files**: `decisions`, `implementation`, `verification`, `follow-ups` and
  `blockers`, at their configured paths. Each is a title and the empty structure its rules read (the
  index's table header; the record files' titles). None of these seeds carries a marker line
  (Decided, 2). The seeded ledger is structurally complete but not content-complete: see "An empty
  ledger", below.
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

**The rule files** (`<root>/rules/*.md`) are not seeded (Decided, 1). A valid rule file needs an
`area` and an `owner` that names a decision of the project's ledger (`agents/rule-owner`), and no
generic seed has either. T-061's `#input` at `<root>/rules` stays the one finding. Its message says
what to write.

## 3. What one `gate sync` gives a service

With `docs/layout`, `ledger/layout` and `agents/agents-block` in `[generate]`, on an empty project:

| Plugin | Seeds | What `gate check` reports afterwards |
| --- | --- | --- |
| `docs` | the 9 indexes (T-062) | `docs/layout#unfilled` per index until its marker line is deleted; no `#index` finding, because the seeded table lists the `delegated` areas (Decided, 3) |
| `ledger` | `manifest.json` and the five shared files | `#input` for the files only the project writes (the ADR coverage, the catalog of `ledger/reuses`); `ledger/records#record-shape` keyed `implementation record#none` and `production blocker record#none` |
| `agents` | `AGENTS.md` with its block | `#input` at `docs/agents/rules` from each rule-file rule (T-061); `gate sync` and `gate sync --check` report the same missing rule files as `gate/sync#agents/agents-block` |

## 4. Tests

- `ledger/layout`, native:
  - on an empty project it seeds the manifest and the five files at the defaults, and at configured
    paths when the options move them;
  - over the seeded ledger, the ledger rules report only the `#input` of the files the project
    writes and the two `#none` record findings, by key;
  - with a tracked manifest that lists a sixth canonical file, it seeds that file too;
  - every file is a seed.
- `agents/agents-block`, native:
  - with no `AGENTS.md` it returns one seed holding the markers and the block, and no finding;
  - with `AGENTS.md` tracked, its output is as today.
- End to end, once: an empty service with the three generators, the docs, ledger and agents rules
  on, and `delegated = ["agents", "ledger"]`.
  - `gate sync` writes 9 + 6 + 1 files.
  - `gate check` gives exactly the content findings the test lists (`#unfilled` per index, the
    rule-file `#input`, the ledger `#input` for the options T-061 makes required), so a new
    structural finding fails the test.
  - A second `gate sync` writes nothing.
  - `gate sync --check` reports no seed; its one finding is `gate/sync#agents/agents-block` at
    `docs/agents/rules`, the rule files no generator writes (so it exits 1, like `gate sync`).
  - Under `docs/recommended` with the rules at error the check exits 1; under `docs/adoption` with
    the rules at warning it exits 0.

## An empty ledger

The design first said the ledger rules accept an empty ledger. They do not: `ledger/records`
requires at least one implementation record and one production blocker record, and reports each
missing one as `record-shape` keyed `<record>#none`. The lead decided (2026-11-20, option (a)) that
these two are content findings, not structural ones:

- **Structurally complete**: after `gate sync`, every file the ledger rules locate exists at its
  configured path, the manifest names exactly those files under their root and passes
  `ledger/manifest`, and nothing is misplaced or malformed. No rule asks for a file to be moved,
  renamed or reshaped.
- **Not content-complete**: what remains asks the person to write something only they can write,
  as for a docs index's `#unfilled`: the ADR coverage and the catalog (`#input`), and the project's
  first implementation record and first production blocker record (`#none`). A generic record
  would be fake content, so none is seeded, and `ledger/records` is unchanged.

The end-to-end test lists the two `#none` findings in its expected set (by rule, aspect, path and
message; the CLI report carries no key, so the native test pins the keys).

## Decided (lead, 2026-11-20)

1. **Rule files**: no seed. T-061's `#input` at `<root>/rules` stays the one finding.
2. **Ledger seeds**: no marker line. An empty ledger is structurally complete; its two `#none`
   record findings are content findings (see "An empty ledger").
3. **The docs index and the delegated entries**: (d). `docs/layout` takes a plugin option naming
   the delegated areas (`delegated = ["agents", "ledger"]`, empty by default). The seed of
   `docs/README.md` writes a row for each listed area; `docs/layout#index` requires rows for the
   listed areas and for any delegated directory that exists; a listed area whose directory is
   missing is a finding that says to run `gate sync` or remove the entry. A preset carries rule
   options only, not plugin options, so the option is documented on the `docs/layout` page. This
   goes into T-062's review fix round. T-063 only sets it in its end-to-end test.

The bar (lead, corrected): after `gate sync` on an empty project with the docs, ledger and agents
plugins enabled and `delegated` set, `gate check` reports no structural finding: nothing from
`docs/layout#index`, the missing-area finding, or any layout or placement aspect. It reports only
content findings, which ask the person to write something: `docs/layout#unfilled`, and the rule-file
and ledger `#input` findings. A seeded project nobody has written into fails, and only for missing
content: under `docs/recommended` it exits 1 by design; under `docs/adoption` it exits 0.

## Done when

- The code goes on a tip that carries T-061 and T-062,
  followed by one lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`),
  deny and the release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Resume point

2026-11-20: design written on `task/T-063-seeds` from T-062's tip `ec96a38`, read against T-061's
branch (its `input` aspect, the `agents-block` finding, `ledger/manifest`'s fields and default path).
No code yet: T-061 sits on T-059's tip and T-062 on `main`, so the code waits for a tip that carries
both. Waiting for the lead's decision on the open items.

2026-11-20: the lead decided the three items (see Decided). The `delegated` option goes into T-062's
review fix round.
2026-11-20: the lead corrected the bar: no structural finding after sync; only content findings.
2026-11-20: the lead accepted the working base (T-062 `7fb79d5` merged with T-061 `6a286e2`,
conflicts in `CHANGELOG.md`, `docs/guide/plugins.md` and `docs/plan.md` resolved as unions). Code
written on it: `ledger/layout`, the `AGENTS.md` seed, native tests, the three-plugin CLI test, pages
and changelogs. `gate sync --check` cannot exit 0 on an empty project: the missing rule files stay
its one finding, which the test lists.

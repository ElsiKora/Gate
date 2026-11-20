# T-061: An enabled doc rule fails on a missing ledger or agent docs

| Field | Value |
| --- | --- |
| Status | approved as designed (81053fd) with the lead's rulings; code on T-059's tip, in review after a green round |
| Release | v0.3.0 (both plugins release with it) |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | T-059 (both touch `plugins/ledger`): the code is built on T-059's tip `21d2d53` (T-059 rebased onto main at `e9e1434`), as T-052 was on T-048, and is rebased when T-059 merges |
| Touches | `plugins/ledger` (every rule that reads a file, three option types, the rule pages), `plugins/agents` (the rules that read the rule files or the entry, the `agents-block` generator, their pages), tests, `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

Today a project with no ledger passes every ledger rule: each one "reports nothing while `<file>` is
not tracked". Three rules are also silent when an option is unset (`closing-notes` without `field`,
`reuses` without `catalog`, `record-status` without `parity`). Nothing requires `AGENTS.md` or
`docs/agents` either. The owner's requirement: a project that turns these plugins on and has no
ledger, or only part of one, fails. Turning a rule on must never leave a silent hole.

## 1. What "the ledger is present" means, and where that list lives

The list lives in one place: **the ledger manifest**. Its `canonicalFiles` (below `canonicalRoot`)
and its `idFamilies` (each with a `file` or a `directory`) say what a complete ledger is, and
`ledger/manifest#manifest-file` already reports each one that is not tracked. There is no plugin
option that lists required files, because that would be a second list.

The manifest has to be the list because a real ledger is mostly the project's own files. A near-complete ledger read for this design lists about thirty canonical files and nine identifier families. The plugin's five default files are among them. The rest are the project's own (decision files per area, plans, templates, registers), and no plugin default could name them. One of its families even points into the agent rule directory. The manifest is also part of the ledger, so it travels with the ledger it describes.

What changes is that the manifest itself, and every file a rule reads, can no longer be absent
silently. That is the subject of question 2. A rule's path options say where the rule reads; they
are not a list of required files. A file that no rule reads is required only by the manifest.

A complete ledger at the plugin's defaults, the synthetic example used in docs and tests:

```json
{
  "canonicalRoot": "docs/ledger",
  "canonicalFiles": ["decision-index.md", "implementation-slices.md", "verification.md",
                     "follow-ups.md", "production-blockers.md"],
  "idFamilies": {
    "D": { "pattern": "`D-[A-Z]+-\\d{3}`", "file": "decision-index.md" },
    "I": { "pattern": "`I-[A-Z]+-\\d{3}`", "file": "implementation-slices.md", "unique": true },
    "V": { "pattern": "`V-[A-Z]+-\\d{3}`", "file": "verification.md", "unique": true }
  },
  "forbiddenReferences": []
}
```

## 2. How a project opts in

There is no opt-in step. A missing input is a finding of every rule that reads it, by default, with
no option. Of the two forms proposed, neither:

- A new rule (`ledger/present`) leaves the hole: a project that turns on `ledger/records` and not
  `ledger/present` still passes with no ledger. This is the hole the owner wants closed.
- An option that turns "reports nothing" into a finding is either off by default, which is the
  hole again, or on by default, which makes it an escape hatch. Two escape hatches already exist and
  are explicit: an `input` level of `off`, and an exception.

So:

- **Missing files.** Each rule that reads a file reports that file, when it is not tracked, as a
  finding of a new aspect `input`. The finding is at the path, keyed by it, and names the option
  that points there. The message reads like this: "docs/ledger/implementation-slices.md is not
  tracked; ledger/records reads it (`[plugins.ledger] options` `implementation`): add it, or point
  the option at it".

  | Rule | Inputs reported when not tracked |
  | --- | --- |
  | `ledger/manifest` | `manifest` (ruling 1: a missing manifest is `ledger/manifest#input`) |
  | `ledger/decisions` | the shared `decisions`, and `coverage` |
  | `ledger/records`, `ledger/record-status`, `ledger/blockers`, `ledger/closing-notes`, `ledger/reuses` | the shared `implementation`, and with `parity` its `plan` |
  | `ledger/reuses` | also `catalog`, and the shared `decisions` |

  Each message names the missing file and the option, and says what to do (ruling 3):
  "docs/ledger/implementation-slices.md is not tracked, and ledger/records reads it
  ([plugins.ledger] options implementation); create it, point the option at the file, or turn the
  input off with a decision: "ledger/records" = { levels = { input = "off" }, decision = "<record>" }".

  One missing file can produce one finding per rule that reads it. That is honest, because each of
  those rules could not run, and adding the file clears them all. Making one rule own the report
  instead would bring the hole back whenever that rule is off.

- **Options that switch a rule off.** An option that, when unset, makes an enabled rule do nothing
  becomes required. Leaving it out is a configuration error (exit 2) at load, not a silent pass:
  - `field` of `ledger/closing-notes` and `catalog` of `ledger/reuses` lose their empty default and
    become required in the options schema.
  - The shared `parity` becomes a need of `ledger/record-status`, declared through `x-gate-needs`
    (`Rule::NEEDS`). The host already refuses an enabled rule whose needed key is missing.

- **Unchanged.** `ledger/exception-owners`, `ledger/exception-decisions` and `ledger/rule-decisions`
  stay silent without an exceptions file or without decisions: no exceptions is a complete state,
  not a missing one. Without an index they already report every named decision as unlisted.

- Every page drops its "reports nothing while ... is not tracked" sentence and documents the `input`
  aspect.
- `CHANGELOG.md` gets a Breaking Changes entry for 0.3.0: a project that turned a rule on without its
  files now fails. Scope profiles to the projects that have a ledger, or set `input` to `off`.
- `input` is a new aspect, so `levels = { input = "warning" }` and exceptions work on it as on any
  aspect.

## 3. The agents plugin

Recommendation: **yes, by the same principle.** An enabled agents rule that cannot run fails.

- The rule files: `agents/rule-registry`, `agents/rule-ids`, `agents/rule-file-size`,
  `agents/rule-enforcement`, `agents/rule-owner` and `agents/rule-references` read
  `<root>/rules/*.md` (`root` defaults to `docs/agents`). With no rule file there, each reports
  `#input` once, at `<root>/rules`.
- The entry: `agents/instruction-references` reports `entry` (`AGENTS.md`) when it is not tracked.
  Today it reads the entry only "when it is tracked". The `agents/agents-block` generator, which
  returns silently today, reports it as its finding (`gate/sync#agents-block`): "AGENTS.md is not
  tracked; create it with the `gate:begin agents` and `gate:end agents` lines". The generator
  writes only between the markers; it cannot create the file. It also reports an empty
  `<root>/rules`. Both are `gate sync` findings, in `gate sync` and in `gate sync --check`, and the
  message says to remove the generator from `[generate]` when the project does not want the block.
- Unchanged:
  - `agents/skills`, since a project without skills is complete;
  - `agents/no-claude-md`, which reads no input;
  - the `agents/adapters` generator, which with no rule file writes no adapter, and whose missing
    input `agents/rule-registry` reports.

## Presets (ruling 2)

No preset turns on `ledger/closing-notes`, `ledger/reuses` or `ledger/record-status`: neither the
ledger plugin nor the agents plugin declares a preset (`presets: []` in both). A future preset
leaves these three rules out, because their `field`, `catalog` and `parity` are the project's own
and have no sane default. A preset that enables the other ledger or agents rules on a fresh project
then gives `#input` findings that name the files to create, never exit 2 for an option the user did
not know about. `docs/guide/plugins.md` says so under Presets.

## 4. The reference read

The read was READ-ONLY. It was limited to `git ls-tree` and `git show HEAD:<file>` of the ledger
manifest, the ledger and agent directory listings, and the old gate configuration. No checkout,
fetch, build or script was run. Nothing of it is in this repository except what question 1 says in
general terms: the manifest is the list a real ledger keeps, the plugin's default files are a subset
of it, and agent docs are an entry file plus a rule directory with an optional skill directory. Docs
and tests use only the plugin's defaults and the synthetic manifest above.

## 5. Tests

These are the cases that would let a broken change through, or block a good one. They are native
SDK tests per rule, plus one end-to-end run per plugin through the built components.

- Ledger, absent, with every ledger rule enabled on an empty project:
  - each rule that reads a file reports `#input` for it, at its path;
  - the exception and rule-decision rules report nothing;
  - the run exits 1.
- Ledger, partial:
  - With the manifest and the index tracked but `implementation-slices.md` missing:
    `ledger/records#input` and `ledger/manifest#manifest-file` report it, and
    `ledger/decisions#input` reports the missing `coverage`.
  - With every rule input tracked but a canonical file the manifest lists missing: only
    `#manifest-file` reports it.
- Ledger, complete: the synthetic ledger at the defaults with the manifest above gives no finding and
  exits 0.
- Ledger, options:
  - `ledger/closing-notes` without `field`, `ledger/reuses` without `catalog`, and
    `ledger/record-status` without `parity` each exit 2, naming the key.
  - `levels = { input = "off" }` turns the input findings off, and only them.
- Agents, absent: `agents/rule-registry` (and each rule-file rule) reports `#input` at `docs/agents/rules`;
  `agents/instruction-references` reports `AGENTS.md`; `gate sync --check` reports the
  `agents-block` finding; the check exits 1.
- Agents, partial: `AGENTS.md` with the markers and no rule file reports only the rule-file inputs; a
  rule file and no `AGENTS.md` reports only the entry. A missing `skills` directory reports nothing.
- Agents, complete: `AGENTS.md` with the markers and one valid rule file gives no finding, and `gate
  sync --check` reports no drift.
- Keys: an `input` finding's key is its path, so moving it within its file never changes its
  fingerprint.

## Done when

- The lead decides. Then the code goes on T-059's tip, followed by one lane round on the pushed tip:
  fmt, clippy, test (`--workspace --no-fail-fast`), deny and the release build exit 0, and the leak
  scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Resume point

2026-11-19: design written from `main` at `ec66737` and approved. Code written on T-059's tip:
`plugins/ledger/src/input.rs` and the `input` aspect on every ledger rule that reads a file; the
required `field`, `catalog` and `parity`; the agents rules and the `agents-block` generator; native
tests (`plugins/ledger/tests/rules/presence.rs` for an absent, partial and complete ledger) and
end-to-end tests in `crates/gate/tests/plugins.rs`; the rule pages, the guide and the CHANGELOG.
Round 263 (ticket 000000000263, on `2cb3001`) stopped at clippy: `too_many_lines` in
`instruction-references` (105/100); fixed by moving the command pattern and the files read into
functions of their own. Rebased onto T-059 at `21d2d53`; the `agents()` test helper now uses the
memoized `component()` that main's T-060 brought. Next: the lane round on the pushed tip, then
review.

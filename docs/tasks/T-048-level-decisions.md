# T-048: A rule set below its default names its decision

| Field | Value |
| --- | --- |
| Status | in progress (design approved by the lead, 2026-11-19) |
| Release | v0.3.0 (it renames a configuration table; with T-052) |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | `main` at `7f62554` (T-073 merged); merges as one batch with T-059, T-061 and T-052, which are built on it |
| Touches | `crates/gate-host/src/config` (the rule entry, the policy table, settings), `crates/gate-host/src/workspace.rs` (the exception check), `crates/gate-host/src/plugins.rs` (the load-time check beside `Rule::NEEDS`), tests, docs, `CHANGELOG.md` |

## Goal

A consumer that adopts Gate under its own governance needs every relaxation of a rule to name the
decision that accepted it: an exception already can and, under a policy, must; a rule entry that sets
a rule or an aspect below its default cannot, because `decision` on a rule entry is an unknown field
(exit 2).

## Design

One mechanism for both relaxations. A rule entry's table form takes `decision`, in the form an
exception's takes it: a non-empty string, trimmed, the id of the decision record; an empty one is a
configuration error. The policy has one place: `[exception-policy]` becomes `[decision-policy]`,
with its one key, `require-decision = true`, meaning that every relaxation names its decision: each
`[[exception]]`, and each rule entry (a rule, or a `"<plugin>/*"` family, which takes `decision`
beside its level and nothing else) whose `level`, or any level in `levels`, is below its default.
The default is the level a preset gives the rule or aspect: the preset's level of that aspect, else
of the rule; below means a lower level (`error` > `warning` > `note`) or `off`. A rule no preset
names has no default: it runs only because its entry or a family turns it on, which is stricter than
not running, so it needs no decision at any level (lead's ruling on the review, 2026-11-19). An
entry's whole-rule `level` is compared with the preset's rule level and with each preset aspect
level it overrides (those `levels` does not name), since it replaces them all. Scope is as today:
the root's policy covers every file, and a project's own `gate.toml` may set its own for its
exceptions and its rule entries; `[[profiles]]` are the root's. A decision covers the levels of the
entry that writes it, and travels with them when files layer, so a project file that lowers a rule
the root left alone needs its own decision. Because presets come from plugins, the check runs when
the plugins load, beside the `Rule::NEEDS` check: a configuration error, exit 2, naming the file and
line, the key as written (`"<plugin>/*"` for a family), the level, the default and the preset that
sets it ("the preset nestjs/recommended sets nestjs/x#y to error"), the policy that requires it, and
the form that adds the decision (the table form for a string entry). With no policy, or
`require-decision = false`, nothing is checked and `decision` is accepted anyway.
`[exception-policy]` is then an unknown table like any other, with the usual list of known tables
and no word about the old name (lead, 2026-11-19: naming it would be a reader of an old
configuration); the CHANGELOG entry under Breaking says the table is renamed and what it now covers.

## Open items

- `ledger/exception-decisions` checks only the decisions of exceptions against the decision index.
  Checking a rule entry's decision there needs the plugin interface to carry rule decisions; not a
  task yet (lead, 2026-11-19).

## Tests

- At load, under `require-decision = true`, for `level` and for `levels`, with a preset: a lowering
  entry without a decision exits 2 with the message above; the same entry with a decision loads; an
  entry at or above its default loads without one. An aspect the preset does not name falls back to
  the preset's rule level.
- Without a preset that names the rule, every level loads, the rule's and a family's.
- `off` is below every default; a family below a rule's default needs a decision, and the refusal
  names the family key.
- A whole-rule `level` equal to the preset's rule level but below a preset aspect level it overrides
  needs a decision.
- A project file that lowers what the root set (and decided) needs its own decision. A project's own
  `[decision-policy]` governs its file under a root that requires decisions: `false` loads, `true`
  refuses and names the project's file; the root's entries stay under the root's policy.
- The policy off (absent, or `false`): the same entries load.
- An empty decision on a rule entry exits 2. `[exception-policy]` exits 2 as an unknown table.
- The exception tests of today, moved to `[decision-policy]`.

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the
  release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md.

## Resume point

2026-11-20T17:58:09+00:00: rebased onto `main` at `7f62554`. Main's `check_presets` reads
`self.declarations`, and `presets()` takes a `&PluginInfo`, so `check_decisions` walks the
declarations too and runs for every declared plugin, whether or not the run loads it. The entries
are in the 0.3.0 CHANGELOG section. T-059, T-061 and T-052 are stacked on this branch in that
order, and one lane round on T-052's tip counts for all four; next: that round.

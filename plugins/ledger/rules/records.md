# `ledger/records`

Ledger records keep their fields, counterparts, commits, dependencies and closure evidence.

## What it checks and why

A record that lost a field, a closed record without evidence, or a dependency on a record that does
not exist makes the ledger say something false about the state of the work. Records are read as the
[plugin page](README.md#records) describes.

- `input`: a file the rule reads that is not tracked (`[plugins.ledger] options` `implementation` and, with `parity`, the plan `parity.plan`), reported at
  its path; the rule checks nothing else until it is. A project without its ledger fails the rule:
  create it, point the option at it, or turn the input off with `levels = { input = "off" }` and a `decision`.

- `record-shape`: a collection without a record of its kind, a line that looks like a record but does
  not parse, a record without a title. `record-unique`: an identifier defined twice in a collection.
- `record-field`: a required field missing, repeated or empty (`Status`, `Blocked by`, `Commit`,
  `Owning repository` on paired records; `Phase`, `Status`, `Exit evidence`, `Prerequisite links`,
  `Owner`, `Ledger links`, `Exit criteria` on production blockers), a `Status` or `Phase` that is not
  one backticked value, an optional field repeated, a phase outside `blocker-phases`.
- `record-dependency`: a `Blocked by` or `Prerequisite links` entry that names an unknown or repeated
  record, a paired `Blocked by` that is not `none` or a list of backticked `I-`/`F-` identifiers, and
  plan and implementation `Blocked by` sets that differ. `dependency-cycle`: a cycle of `Blocked by`
  reachable from a paired record.
- `record-closure`: a closed implementation record whose verification notes are a placeholder or whose
  dependency is open; a production blocker with status `closed` whose `Exit evidence` is a placeholder
  or whose prerequisite is open.
- `record-commit`: a closed paired record without a backticked 40-hex `Commit` in plan and
  implementation, plan and implementation commits that differ, and, with `unfinished-commit-pending`,
  an unfinished paired record whose commit is not `pending`.
- With `[plugins.ledger] options` `parity`: `record-counterpart` for a plan heading without its
  implementation or verification record and the reverse, and an implementation record whose
  `Verification items` is not its own `V-` record; `parity-fields` for an active plan record (status
  `active-status`) without `reference` (a backticked full commit id) and each of `fields`.

A value is a placeholder when, with links reduced to their text, markup and trailing `.`, `;` and `:`
removed, white space collapsed and in lowercase, it is one of `pending-values` or matches one of
`pending-patterns`. Statuses are compared with `closed-statuses` in lowercase, except for
`unfinished-commit-pending`, which compares them as written. The status agreement of paired records
is `ledger/record-status`.

Scope: project. Reads history: no. Aspects: `dependency-cycle`, `input`,
`parity-fields`, `record-closure`, `record-commit`, `record-counterpart`, `record-dependency`,
`record-field`, `record-shape`, `record-unique`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `pending-values` | array of string | `"", "later", "n/a", "none", "not available", "not provided", "not recorded", "not run", "not yet available", "not yet provided", "not yet recorded", "pending", "placeholder", "tbd", "todo", "unknown"` | Normalized values that are placeholders. |
| `pending-patterns` | array of string | four patterns: `pending`, `not run` and the like as the first word; `placeholder`, `tbd`, `todo`; `to be` or `will be available`; `commit`, `evidence`, `notes` or `verification` followed by `pending` | Patterns that make a normalized value a placeholder; `\d`, `\w` and `\b` are ASCII. |
| `blocker-phases` | array of string | `[]` | The phases a production blocker may have; empty allows any. |
| `unfinished-commit-pending` | boolean | `false` | An unfinished paired record keeps `Commit` `pending` in plan and implementation. |

```toml
[rules]
"ledger/records" = { level = "error", options = { blocker-phases = ["LAUNCH", "RAMP"] } }
```

## Example finding

`docs/ledger/verification.md`, for a closed implementation record `I-CORE-API-001`:

```markdown
- `V-CORE-API-001`: API checks
  - Status: `completed`
  - Verification notes: TBD
```

```text
error ledger/records#record-closure docs/ledger/verification.md:1
  docs/ledger/verification.md V-CORE-API-001 is closed without concrete verification notes
```

## How to fix it

Add the missing record or field, write the concrete evidence (the command and its result, the commit
id), name only records that exist, and break dependency cycles.

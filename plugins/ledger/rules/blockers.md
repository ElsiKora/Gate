# `ledger/blockers`

A planned record names no closed blocker, and a started record no blocker.

## What it checks and why

`Blocked by` says what a record still waits for. A closed record there is stale; a started record
that still waits for something is not started. The records are the implementation and verification
records and, with `parity`, the plan headings; a record without a status or a `Blocked by` field
passes, since `ledger/records` owns required fields.

- `input`: a file the rule reads that is not tracked (`[plugins.ledger] options` `implementation` and, with `parity`, the plan `parity.plan`), reported at
  its path; the rule checks nothing else until it is. Create it, point the option at it, or turn the input off with `levels = { input = "off" }` and a `decision`.

- `closed-blocker`: a planned record (status starting with one of `planned-prefixes`) whose
  `Blocked by` names, anywhere in its value, an `I-`, `V-` or `F-` record that is closed: its status, or
  the first word of it, is one of `closed-statuses`. An unknown record passes.
- `started-blocker`: a started record (status starting with one of `started-prefixes`) whose
  `Blocked by` is not `none`.

Scope: project. Reads history: no. Aspects: `closed-blocker`, `input`, `started-blocker`.

## Options

None of its own; it reads `implementation`, `verification`, `follow-ups`, `parity`,
`closed-statuses`, `planned-prefixes` and `started-prefixes` from `[plugins.ledger] options`.

```toml
[plugins.ledger]
source = "ElsiKora/Gate:ledger@0.3.1"
options = { planned-prefixes = ["planned", "queued"] }
```

## Example finding

```markdown
- `I-API-002`: Pagination
  - Status: `planned`
  - Blocked by: `I-API-001` (merged last week)
```

with `I-API-001` `` `completed` ``:

```text
error ledger/blockers#closed-blocker docs/ledger/implementation-slices.md:1
  docs/ledger/implementation-slices.md I-API-002 is planned, but its Blocked by names I-API-001, which is `completed`; keep only open blockers there and move the context to a Dependency context field
```

## How to fix it

Remove closed records from `Blocked by` and keep their context in another field. For a started record,
set `Blocked by` to `none` and record what it still waits for in a dated `Completion dependency`.

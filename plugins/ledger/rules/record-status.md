# `ledger/record-status`

A paired plan record, its implementation and its verification share one allowed status.

## What it checks and why

With `[plugins.ledger] options` `parity`, each plan heading `I-<prefix>-…` pairs with an
implementation record of the same identifier and a verification record `V-<prefix>-…`. When their
statuses differ, the ledger reports the same work as done in one place and open in another.

- Each plan heading whose three records all have a status: the three statuses are equal, as written.
- With `allowed`: each status of the plan heading and its two records is one of `allowed`, as written;
  one finding per file.

Without `parity`, or while `implementation` is not tracked, the rule reports nothing. Fields and
counterparts are `ledger/records`.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `allowed` | array of string, not empty | absent: any status | The statuses a paired record may have. |

```toml
[rules]
"ledger/record-status" = { level = "error", options = { allowed = ["planned", "in progress", "completed"] } }
```

## Example finding

A plan heading `### I-CORE-API-001: API` with status `` `completed` `` whose verification record has
status `` `passed` ``:

```text
error ledger/record-status docs/ledger/plans/core.md:3
  I-CORE-API-001 and V-CORE-API-001 must have exactly matching Status values
```

## How to fix it

Give the three records the status the work has, from the allowed list.

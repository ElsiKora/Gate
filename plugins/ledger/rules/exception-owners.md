# `ledger/exception-owners`

The owner an exception names is an open implementation record.

## What it checks and why

An `[[exception]]` in `gate.exceptions.toml` may name the implementation record that owns it, the work
that will remove the accepted finding. An owner that does not exist, or whose record is not open, leaves
the exception without work in progress to remove it.

- Each exception with `owner`: the owner is an implementation record (`[plugins.ledger] options`
  `implementation`) whose status is open: it starts with one of `planned-prefixes` or
  `started-prefixes` (by default `planned`, `in progress`, `in-progress`). A record with no status, or
  with any other status (`completed`, `blocked`, `rejected`, ...), is reported.
- The finding points at the exception's `[[exception]]` line.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `exceptions` | string | `"gate.exceptions.toml"` | The project's exceptions file. |

```toml
[rules]
"ledger/exception-owners" = "error"
```

## Example finding

```toml
[[exception]]
rule = "demo/rule"
target = "src/a.ts"
reason = "kept until the migration"
owner = "I-API-001"
```

with `I-API-001` `` `completed` ``:

```text
error ledger/exception-owners gate.exceptions.toml:1
  the exception for demo/rule names the owner I-API-001, which is `completed`; only an open record, planned or in progress, owns an exception, so fix the finding, or move the exception to the open record that owns the work
```

## How to fix it

Fix the accepted finding and remove the exception, or name the open record that will.

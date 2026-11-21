# `ledger/root-exception-owners`

The owner a root exception names is an open implementation record.

## What it checks and why

In a workspace whose root is not a project, the root `gate.exceptions.toml` holds the exceptions of
the repository-scope rules. An exception there may name the implementation record that owns it, like
a project's. This rule holds the root exceptions to the same judgement as
[`ledger/exception-owners`](exception-owners.md) holds a project's.

- Each exception with `owner` in the exceptions file beside the root `gate.toml`: the owner is an
  implementation record (`[plugins.ledger] options` `implementation`, relative to the repository
  root) whose status is open, as `ledger/exception-owners` decides it.
- The messages are those of `ledger/exception-owners`, and the finding points at the exception's
  `[[exception]]` line.
- When the root is a project, the root exceptions file is that project's, `ledger/exception-owners`
  checks it, and this rule reports nothing: no exception is judged twice.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `exceptions` | string | `"gate.exceptions.toml"` | The root's exceptions file, relative to the directory of the root `gate.toml`. |

```toml
[rules]
"ledger/root-exception-owners" = "error"
```

## Example finding

A root that is not a project, with `I-API-001` `` `completed` `` in the implementation records, and
in the root `gate.exceptions.toml`:

```toml
[[exception]]
rule = "demo/whole"
target = "README.md"
reason = "kept until the migration"
owner = "I-API-001"
```

```text
error ledger/root-exception-owners gate.exceptions.toml:1
  the exception for demo/whole names the owner I-API-001, which is `completed`; only an open record, planned or in progress, owns an exception, so fix the finding, or move the exception to the open record that owns the work
```

## How to fix it

Fix the accepted finding and remove the exception, or name the open record that will.

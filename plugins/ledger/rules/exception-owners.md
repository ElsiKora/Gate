# `ledger/exception-owners`

The owner an exception names is an open implementation record.

## What it checks and why

An `[[exception]]` in a `gate.exceptions.toml` may name the implementation record that owns it, the
work that will remove the accepted finding. An owner that does not exist, or whose record is not open,
leaves the exception without work in progress to remove it.

- Every exception with `owner`, in every `gate.exceptions.toml` of the workspace: the root's and each
  project's, a nested project's included, whether or not the root is a project. The rule runs in the
  repository call and learns the files from the core, so it judges exactly the files the core reads
  (none under `[workspace] ignore`), each exception once.
- The owner is an implementation record (`[plugins.ledger] options` `implementation` of the root
  `gate.toml`, relative to the repository root) whose status is open: it starts with one of
  `planned-prefixes` or `started-prefixes` (by default `planned`, `in progress`, `in-progress`). A
  record with no status, or with any other status (`completed`, `blocked`, `rejected`, ...), is
  reported. A nested project's own records file is not read.
- The finding points at the exception's `[[exception]]` line, in the file that holds it.
- The decision an exception names is [`ledger/config-decisions`](config-decisions.md)'s.

Scope: repository. Reads history: no. Aspects: none.

## Options

None. Turn it on in the root `gate.toml`; a project that turns it on while the root never names it is
a configuration error.

```toml
[rules]
"ledger/exception-owners" = "error"
```

## Example finding

In a nested project's `infra/cdk/gate.exceptions.toml`:

```toml
[[exception]]
rule = "demo/rule"
target = "src/a.ts"
reason = "kept until the migration"
owner = "I-API-001"
```

with `I-API-001` `` `completed` `` in the root's implementation records:

```text
error ledger/exception-owners infra/cdk/gate.exceptions.toml:1
  the exception for demo/rule names the owner I-API-001, which is `completed`; only an open record, planned or in progress, owns an exception, so fix the finding, or move the exception to the open record that owns the work
```

## How to fix it

Fix the accepted finding and remove the exception, or name the open record that will.

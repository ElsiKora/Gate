# `ledger/root-exception-decisions`

The decision a root exception names is an accepted decision of the index.

## What it checks and why

In a workspace whose root is not a project, the root `gate.exceptions.toml` holds the exceptions of
the repository-scope rules. An exception there may name the decision that accepted it, like a
project's. This rule holds the root exceptions to the same judgement as
[`ledger/exception-decisions`](exception-decisions.md) holds a project's.

- Each exception with `decision` in the exceptions file beside the root `gate.toml`: the decision
  index (`[plugins.ledger] options` `decisions`, relative to the repository root) lists it with the
  status `accepted`.
- The judgement and the messages are those of `ledger/exception-decisions`: an exception without
  `decision` is not this rule's finding, and the finding points at the exception's `[[exception]]`
  line.
- When the root is a project, the root exceptions file is that project's,
  `ledger/exception-decisions` checks it, and this rule reports nothing: no exception is judged
  twice.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `exceptions` | string | `"gate.exceptions.toml"` | The root's exceptions file, relative to the directory of the root `gate.toml`. |

```toml
[rules]
"ledger/root-exception-decisions" = "error"
```

## Example finding

A root that is not a project, with `D-API-002` `` `proposed` `` in the index, and in the root
`gate.exceptions.toml`:

```toml
[[exception]]
rule = "ledger/root-rule-decisions"
target = "gate.toml"
reason = "kept for now"
decision = "D-API-002"
```

```text
error ledger/root-exception-decisions gate.exceptions.toml:1
  the exception for ledger/root-rule-decisions names the decision D-API-002, which is `proposed`; an exception needs an accepted decision
```

## How to fix it

Accept the decision in the index, name the accepted decision that covers the exception, or fix the
finding and remove the exception.

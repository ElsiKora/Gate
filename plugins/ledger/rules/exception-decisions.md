# `ledger/exception-decisions`

The decision an exception names is an accepted decision of the index.

## What it checks and why

An `[[exception]]` may name the decision that accepted it. A decision that the index does not list, or
that is not accepted, does not justify the exception.

- Each exception with `decision`: the decision index (`[plugins.ledger] options` `decisions`) lists it
  with the status `accepted`.
- An exception without `decision` is not this rule's finding. Whether every exception must name one
  is `[decision-policy] require-decision` in `gate.toml`, which the host enforces when it loads the
  configuration.
- The finding points at the exception's `[[exception]]` line.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `exceptions` | string | `"gate.exceptions.toml"` | The project's exceptions file. |

```toml
[rules]
"ledger/exception-decisions" = "error"
```

## Example finding

```toml
[[exception]]
rule = "demo/rule"
target = "src/a.ts"
reason = "kept until the migration"
decision = "D-API-002"
```

with `D-API-002` `` `proposed` ``:

```text
error ledger/exception-decisions gate.exceptions.toml:1
  the exception for demo/rule names the decision D-API-002, which is `proposed`; an exception needs an accepted decision
```

## How to fix it

Accept the decision in the index, name the accepted decision that covers the exception, or fix the
finding and remove the exception.

# `ledger/rule-decisions`

The decision a rule entry names is an accepted decision of the index.

## What it checks and why

A rule entry, or a `"<plugin>/*"` family, may name the decision that accepted its levels. A decision
that the index does not list, or that is not accepted, does not justify the entry.

- Each decision a rule entry of the project's own `gate.toml` names, in `[rules]` and in the `rules`
  of each `[[profiles]]` entry: the decision index (`[plugins.ledger] options` `decisions`) lists it
  with the status `accepted`. The root `gate.toml` is the root project's own file, when the top is a
  project.
- Every written decision is checked, whether or not a later layer overrides the entry and whether or
  not a policy requires it.
- An entry without `decision` is not this rule's finding. Whether an entry below its default must
  name one is `[decision-policy] require-decision` in `gate.toml`, which the host enforces when it
  loads the plugins.
- The core hands the rule the decisions; the plugin does not read `gate.toml`.
- The finding points at the entry's line and names its key as written.

Scope: project. Reads history: no. Aspects: none.

## Options

None.

```toml
[rules]
"ledger/rule-decisions" = "error"
```

## Example finding

```toml
[rules]
"demo/rule" = { level = "off", decision = "D-API-002" }
```

with `D-API-002` `` `proposed` ``:

```text
error ledger/rule-decisions gate.toml:2
  the rule entry "demo/rule" names the decision D-API-002, which is `proposed`; a rule entry needs an accepted decision
```

## How to fix it

Accept the decision in the index, name the accepted decision that covers the entry, or restore the
entry's level and remove the decision.

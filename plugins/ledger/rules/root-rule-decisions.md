# `ledger/root-rule-decisions`

The decision a root rule entry names is an accepted decision of the index.

## What it checks and why

In a workspace whose root is not a project, the root `gate.toml` turns on the repository-scope rules
and sets the projects' levels in `[[profiles]]`. Its entries may name the decision that accepted
their levels, like a project's. This rule holds the root file to the same judgement as
[`ledger/rule-decisions`](rule-decisions.md) holds a project's own file.

- Each decision a rule entry of the root `gate.toml` names, in `[rules]` and in the `rules` of each
  `[[profiles]]` entry: the decision index (`[plugins.ledger] options` `decisions`, relative to the
  repository root) lists it with the status `accepted`.
- The judgement, the messages, the keys and the aspects are those of `ledger/rule-decisions`: every
  written decision is checked, an entry without `decision` is not this rule's finding, and the
  finding points at the entry's line and names its key as written.
- When the root is a project, the root file is that project's own, `ledger/rule-decisions` checks
  it, and this rule reports nothing: no entry is judged twice.
- The core hands the rule the decisions; the plugin does not read `gate.toml`.

Scope: repository. Reads history: no. Aspects: none.

## Options

None.

```toml
[rules]
"ledger/root-rule-decisions" = "error"
```

## Example finding

A root that is not a project, with `D-API-002` `` `proposed` `` in the index:

```toml
[workspace]
projects = ["apps/*"]

[rules]
"ledger/root-rule-decisions" = "error"
"ledger/root-exception-owners" = { level = "off", decision = "D-API-002" }
```

```text
error ledger/root-rule-decisions gate.toml:6
  the rule entry "ledger/root-exception-owners" names the decision D-API-002, which is `proposed`; a rule entry needs an accepted decision
```

## How to fix it

Accept the decision in the index, name the accepted decision that covers the entry, or restore the
entry's level and remove the decision.

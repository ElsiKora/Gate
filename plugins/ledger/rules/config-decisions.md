# `ledger/config-decisions`

The decision a configuration entry names is an accepted decision of the index.

## What it checks and why

A rule entry, a `"<plugin>/*"` family or an `[[exception]]` may name the decision that accepted it.
A decision that the index does not list, or that is not accepted, does not justify the entry. The
workspace has one decision index, so every decision written anywhere in its configuration is judged
against it, once, by this rule.

- Each decision written in the workspace's configuration, wherever it is written: the `[rules]`
  entries and families of the root `gate.toml` and of every project's `gate.toml`, the `rules` of
  each `[[profiles]]` entry, and each `[[exception]]` of every `gate.exceptions.toml`. The decision
  index (`[plugins.ledger] options` `decisions` of the root `gate.toml`, relative to the repository
  root) lists it with the status `accepted`.
- Every written decision is checked, whether or not a later layer overrides the entry and whether or
  not a policy requires it.
- An entry or exception without `decision` is not this rule's finding. Whether one must name a
  decision is `[decision-policy] require-decision` in `gate.toml`, which the host enforces when it
  loads the configuration and the plugins.
- The finding points at the line of the entry, or of the exception's `[[exception]]`, in the file
  that wrote it, a nested project's file included, and names the entry's key as written (for an
  exception, its `rule`).
- The core hands the rule the decisions in the repository call, and in no project's call; the plugin
  reads neither `gate.toml` nor `gate.exceptions.toml`.

Scope: repository. Reads history: no. Aspects: none.

It runs in the repository call in both shapes of a workspace, so it is turned on in the root
`gate.toml`:

- **The root is a project** (no `[workspace]`, or one that lists `"."`): the root file's `[rules]`,
  next to the root project's own rules. A project nested in it, with its own `gate.toml`, is judged
  by the same run, against the index at the repository root.
- **The root is not a project**: the root file's `[rules]`, or `[workspace] extends` with a ledger
  preset. The root's `[plugins.ledger]` names the index.

A project's own `gate.toml` cannot turn it on: it is a repository-scope rule.

## Options

None. The index is the shared option `decisions` of the root's `[plugins.ledger]`.

```toml
[rules]
"ledger/config-decisions" = "error"
```

## Example finding

A root project with `D-API-002` `` `proposed` `` in its index and a nested project `infra/cdk`,
whose `infra/cdk/gate.toml` is:

```toml
[project]
name = "cdk"

[rules]
"demo/rule" = { level = "off", decision = "D-API-002" }
```

Its `infra/cdk/gate.exceptions.toml` names a decision the index does not list:

```toml
[[exception]]
rule = "demo/rule"
target = "src/a.ts"
reason = "kept until the migration"
decision = "D-NOPE-009"
```

```text
error ledger/config-decisions infra/cdk/gate.toml:5
  the entry for "demo/rule" names the decision D-API-002, which is `proposed`; an entry needs an accepted decision
error ledger/config-decisions infra/cdk/gate.exceptions.toml:1
  the entry for "demo/rule" names the decision D-NOPE-009, which docs/ledger/decision-index.md does not list; name an accepted decision of the index
```

## How to fix it

Accept the decision in the index, name the accepted decision that covers the entry, or restore the
entry's level (or drop the exception) and remove the decision.

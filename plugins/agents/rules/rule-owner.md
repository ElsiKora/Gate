# `agents/rule-owner`

Every rule file owner is an accepted decision.

## What it checks and why

A rule file says which decision made it binding; a rule owned by no decision, or by one that was
never accepted, has no authority. The `owner` of each rule file with front matter must be a decision
of the decision index whose status is `accepted-status`. The index is a Markdown table: every line
that starts with ``| `D-`` and has at least four cells is a row, its first cell the id and its second
the status (a backticked token or the trimmed cell); the first row of an id wins.

Without the index, every owner is reported.

`#input`: `<root>/rules` holds no rule file (`*.md` directly in it), reported once at the directory
and keyed by it; the rule checks nothing else until it does. A project without its agent docs fails
the rule: create a rule file, point `root` at the agent docs, or turn the input off with
`levels = { input = "off" }` and a `decision`.

Scope: project. Reads history: no. Aspects: `input`; the other findings have no aspect.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory of the agent instructions, relative to the project. |
| `decisions` | string | `"docs/ledger/decision-index.md"` | The decision index, relative to the project. |
| `accepted-status` | string | `"accepted"` | The status of a decision that may own a rule file. |

## Example finding

```text
error agents/rule-owner docs/agents/rules/api.md:6
  owner D-API-002 is not an accepted decision in the decision index
```

The finding's target is the rule file's path.

## How to fix it

Name an accepted decision, or accept the decision in the index.

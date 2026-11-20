# `agents/rule-ids`

Rule identifiers are well-formed, unique and never reused.

## What it checks and why

Other documents, reviews and configuration name a rule by its id, so an id must name one rule for
good. Each id is `RULE-<AREA>-<nnn>`: the area of its file and three digits. An id is defined once
across all rule files. The numbers of an area run from 1 to the highest without a gap: a rule that
is no longer wanted keeps its heading with `Status: removed` (and a redirect, if a rule replaces
it), so its number cannot be given to another rule.

The rule reads no history; the gap check is what keeps a deleted id from coming back.

`#input`: `<root>/rules` holds no rule file (`*.md` directly in it), reported once at the directory
and keyed by it; the rule checks nothing else until it does. A project without its agent docs fails
the rule: create a rule file, point `root` at the agent docs, or turn the input off with
`levels = { input = "off" }` and a `decision`.

Scope: project. Reads history: no. Aspects: `input`; the other findings have no aspect.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory of the agent instructions, relative to the project. |

## Example finding

`docs/agents/rules/api.md` defines `RULE-API-001` and `RULE-API-003`:

```text
error agents/rule-ids docs/agents/rules/api.md
  RULE-API-002 is missing; identifiers are never deleted or reused, so keep it with status removed
```

The finding's target is the id. A gap is reported at the first file that defines a rule of the area.

## How to fix it

Put the rule back with `Enforcement: review -- retired · Status: removed`, or rename a new rule to
the next free number of its area.

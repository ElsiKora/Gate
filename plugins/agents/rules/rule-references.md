# `agents/rule-references`

Explicit rule references resolve.

## What it checks and why

A document that cites a rule by id sends the reader to it; a rule that does not exist or was removed
sends them nowhere. The rule reads every tracked `*.md` of the project that contains `RULE-`, and
every inline code span in it that is a well-formed rule id (`` `RULE-API-004` ``) must name a rule
that a rule file defines and that is not `removed`. A reference to a removed rule names its
redirect, if it has one. A redirect on a metadata line must name a defined rule.

Ids in plain text, in headings, in fenced or indented code, or in files that are not Markdown are
not references. A Markdown file that is not UTF-8 does not stop the rule: one without `RULE-` is
skipped, and the others are read with each invalid byte sequence replaced.

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

`docs/guide.md:3` cites `` `RULE-API-002` ``, which is `removed` with a redirect to `RULE-API-001`:

```text
error agents/rule-references docs/guide.md:3
  RULE-API-002 is removed; use RULE-API-001
```

The finding's target is the id.

## How to fix it

Cite the rule that replaces it, or define the rule.

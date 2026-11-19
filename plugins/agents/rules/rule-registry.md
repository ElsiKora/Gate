# `agents/rule-registry`

Rule files declare their area, scope and owner, and every rule has a level-two heading and a
well-formed Enforcement/Status line.

## What it checks and why

Agents load the rule files under `<root>/rules` by their front matter and find a rule by its
heading. A file whose front matter is missing or wrong is loaded for the wrong paths or not at all,
and a rule without its metadata line has no known enforcement or status. The format is the plugin's
own and is not configurable:

```md
---
title: API rules
area: API
applies: globs
globs: ["src/api/**/*.ts"]
owner: D-API-001
---
# API rules

## RULE-API-001 Responses are typed

Enforcement: gate · Status: stable
```

- Front matter: `area` is 2-8 uppercase letters or digits, starting with a letter, and no two files
  declare the same area; `applies` is `always` (and then `globs` is absent or empty) or `globs`
  (and then `globs` is a non-empty list); `owner` is checked by `agents/rule-owner`.
- A rule is a heading whose first word starts with `RULE-`. It must be level two, and the area in
  its id must be the file's area.
- The first non-blank line after the heading is
  `Enforcement: <class>[ <detail>] · Status: <status>[ · Redirect: <id>]`. The classes are `gate`,
  `eslint`, `typecheck`, `test`, `review` and `advisory`; the statuses are `preview`,
  `stable`, `deprecated` and `removed`. A redirect is allowed only on a `removed` or `deprecated`
  rule.
- An `Enforcement:` line outside code fences that follows no rule heading is reported.

A file without valid front matter is reported once and not read further. Rule files are the tracked
`*.md` files directly in `<root>/rules`; a file in a subdirectory is not one.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory of the agent instructions, relative to the project. |

```toml
[rules]
"agents/rule-registry" = { level = "error", options = { root = "docs/agents" } }
```

## Example finding

`docs/agents/rules/api.md` declares `area: api`:

```text
error agents/rule-registry docs/agents/rules/api.md:2
  area api is not 2-8 uppercase letters or digits
```

The finding's target is the rule file's path.

## How to fix it

Correct the front matter or the line the finding names, as in the example above.

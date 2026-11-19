# `agents/rule-file-size`

A rule file has at most `max-lines` lines.

## What it checks and why

An agent loads a whole rule file into its context; a long file crowds out the work. A file above
`max-lines` is reported as `#max-lines`; a file above `warn-lines`, up to `max-lines`, as
`#warn-lines`. A file reports at most one of the two. Give the aspects their own levels, for
example `#warn-lines` as a warning.

Scope: project. Reads history: no. Aspects: `max-lines`, `warn-lines`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory of the agent instructions, relative to the project. |
| `max-lines` | integer | `500` | The most lines a rule file may have. |
| `warn-lines` | integer | `200` | The lines above which a file is reported as `#warn-lines`. Not above `max-lines`: that is a configuration error. |

```toml
[rules]
"agents/rule-file-size" = { levels = { max-lines = "error", warn-lines = "warning" } }
```

## Example finding

```text
warning agents/rule-file-size#warn-lines docs/agents/rules/api.md
  240 lines exceed 200; consider splitting the area
```

The finding's target is the rule file's path.

## How to fix it

Split the area into two rule files, each with an area of its own.

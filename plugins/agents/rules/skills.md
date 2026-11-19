# `agents/skills`

Skills follow the Agent Skills specification.

## What it checks and why

An agent discovers a skill by its `SKILL.md` front matter; a skill that breaks the specification is
skipped or misnamed. Each directory `<root>/skills/<name>/` must hold a `SKILL.md` that opens with
YAML front matter, a mapping whose keys are only `name`, `description`, `license`, `compatibility`,
`metadata` and `allowed-tools`:

- `name` is required, is 1-64 lowercase letters, digits and single hyphens, and equals the
  directory name.
- `description` is required and has 1-1024 characters, not all blank.
- `compatibility`, when set, is a string of at most 500 characters; `metadata` maps strings to
  strings; `license` and `allowed-tools` are strings.

These limits are the specification's and are not configurable.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory of the agent instructions, relative to the project. |

## Example finding

`docs/agents/skills/release/SKILL.md` sets `name: publish`:

```text
error agents/skills docs/agents/skills/release/SKILL.md:2
  name `publish` differs from the directory `release`
```

The finding's target is the file or directory it names.

## How to fix it

Correct the front matter, or rename the directory.

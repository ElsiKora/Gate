# `agents/no-claude-md`

No `CLAUDE.md` is tracked at any level.

## What it checks and why

A tool that reads `CLAUDE.md` in preference to `AGENTS.md` would follow a second, drifting set of
instructions. Every tracked file of the project whose name is `file-name`, compared without regard
to case, is reported. Apply the rule to the project that owns the repository root so that a root
file is seen.

Scope: project. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `file-name` | string | `"CLAUDE.md"` | The file name that may not be tracked. |

## Example finding

```text
error agents/no-claude-md apps/web/CLAUDE.md
  a tracked CLAUDE.md disables AGENTS.md; use AGENTS.md and docs/agents instead
```

The message is fixed text. The finding's target is the file's path.

## How to fix it

Move its instructions into `AGENTS.md` or a rule file, and delete it.

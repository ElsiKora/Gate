# `agents/no-claude-md`

No `CLAUDE.md` is tracked anywhere in the repository.

## What it checks and why

Claude Code reads `AGENTS.md` itself when no `CLAUDE.md` is present, and reads only the `CLAUDE.md`
files when both are. A `CLAUDE.md` that only imports `AGENTS.md` adds nothing; one that says more is
a second set of instructions that other agents never read, and it drifts from `AGENTS.md`. Every
tracked file of the repository whose name is `file-name`, compared without regard to case, is
reported, at any depth.

Scope: repository. Reads history: no. Aspects: none.

A repository-scope rule runs once for the whole repository, so a root `CLAUDE.md` is seen even when
the workspace lists only nested projects. Set it in the root `gate.toml`'s `[rules]`: the core refuses
it in a project's file, as a configuration error.

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

Delete a `CLAUDE.md` that only imports `AGENTS.md`. Move the instructions of any other into
`AGENTS.md` or a rule file, and delete it.

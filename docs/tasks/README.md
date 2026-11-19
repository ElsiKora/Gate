# Tasks

One task is one file here, one branch, one worktree and one writer. The lead writes the file; the writer
keeps its `Status` and `Resume point` current and commits it on the task branch.

File name: `T-NNN-short-slug.md`. Branch: `task/T-NNN-short-slug`.

## Format

```markdown
# T-NNN: <title>

| Field | Value |
| --- | --- |
| Status | planned / in progress / in review / merged / dropped |
| Release | R1 / R2 / ... |
| Writer | <session name> |
| Reviewer | <session name, never the writer> |
| Depends on | T-... or none |
| Touches | the paths this task may change; nothing else |

## Goal
What is true when the task is done, in one paragraph.

## Scope
What to do, as a list. What not to do, as a list.

## Done when
Commands and their expected exit codes; numbers to report.

## Notes
Inputs, pointers, decisions taken on the way (with the reason).

## Resume point
Where the work stands, what comes next. Updated at every push.
```

## Review

The reviewer reads the task file, the diff and the commands' output, runs the `Done when` commands
itself, and answers with findings ranked by severity or "no findings". The writer fixes once; the lead
triages what remains and merges.

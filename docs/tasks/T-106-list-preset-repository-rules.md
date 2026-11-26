# T-106: `gate list` shows the repository-scope rules a preset turns on

| Field | Value |
| --- | --- |
| Status | open |
| Release | the next core release |
| Writer | "Gate T-106" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `80db512` |
| Touches | `crates/gate-host/src/` (the `list` command and the rule set it prints), its tests, `docs/plan.md`, this task file |

## Goal

A configuration extends `ci/recommended` and `git/recommended` and writes no `[rules]` entry for
`ci/permissions`, `ci/runner-pinned`, `git/branch-name` or `git/commit-message`.

`gate check` runs those rules and reports their findings, but `gate list` does not print them. A
`gate check --rule ci/permissions` probe finds them. So the list a user reads to know what runs is
wrong for repository-scope rules that only a preset turns on. Seen on the first consumer with
v0.3.2.

## Required outcome

1. `gate list` prints every rule that `gate check` would run, in every scope, at the level the
   check resolves. A rule that a preset turns on is marked with that preset, as project-scope
   rules are today.
2. The JSON form of `gate list`, if any, matches the text form.
3. `gate list` and `gate check` resolve the rule set from one shared function, so the two cannot
   drift. If they already do, find why the repository scope differs.

## Proof

- A test on a synthetic repository that extends a preset turning on repository-scope rules,
  with no `[rules]` entries: `gate list` shows them with the preset mark, and the set equals the
  rules a `gate check` records as run.
- A rule that a `[rules]` entry sets `off` is not listed as running.
- All cargo runs go through the lane.

## Resume point

Not started.

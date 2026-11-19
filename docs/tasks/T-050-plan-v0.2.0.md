# T-050: The plan and the task files say what v0.2.0 merged

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.2.0 (documents only; no version moves) |
| Writer | "Gate 004: public presentation" |
| Reviewer | the lead reads the documents |
| Depends on | `main` at `4c55ca6` |
| Touches | `docs/plan.md`, `docs/tasks/*.md` (the Status row and the Resume point only) |

## Goal

The plan and the task files are the state of the work, and a reader takes them at their word. At
`v0.2.0` they were not closed: the plan's table stops at T-029 and calls T-025, T-026 and T-029 in
progress, and twenty-three task files still say "in progress", "in review" or "planned" although
their branches merged. A reader who cited them reported merged work as open.

## Scope

1. For each task file on `main`, the Status and the Resume point follow from what `git log
   --first-parent main` shows merged, never from memory: the merge commit, and whether its tip is in
   the `v0.2.0` tag. A task merged before the public root (`59fb163`) is described only as that.
2. `docs/plan.md`: a row for every task from T-030 on that has a task file on `main` or a pushed
   branch, the v0.2.1 work in flight among them (T-037, T-042, T-043, T-044, T-046, T-047); the
   Releases table says what `v0.2.0` released; the items of "Open after rc.1" that merged say so;
   the plan's own resume point is rewritten.
3. T-025 says what is still open: the cost of compiling every configured plugin on a cold run moves
   to T-047.
4. Nothing else in a task file changes: its Goal, Scope and Proof stay as their writer left them.

## Done when

- Every status in the plan and in the task files names the merge it rests on, or the branch tip and
  its own Status for work not merged.
- The leak scan named in the local rules prints nothing.
- The lead has read the documents.

## Proof

The evidence is `git log --first-parent origin/main` at `4c55ca6`, and for each merge `git
merge-base --is-ancestor <merge>^2 v0.2.0`: the branches merged from T-026 (`60651a1`) to T-035
(`89c42c7`, the commit of the `v0.2.0` tag and of the seven plugin tags) are in the tag; T-045,
T-032's proof and T-054 merged after it. Work not merged is read from each pushed branch's own task
file (`git show origin/task/<branch>:docs/tasks/<file>`), at the tips the plan names.

Checks on the result, run in the worktree:

| Check | Command | Result |
| --- | --- | --- |
| every merge the plan and the task files cite is a first-parent commit of `main` | `git log --first-parent --format=%h origin/main`, each cited sha looked up in it | 18 cited, 0 missing |
| every branch tip cited for a merged task is in `main` | `git merge-base --is-ancestor <tip> origin/main` | 16 of 16 |
| no task file but this one still says in progress, in review or planned | `grep -l '^\| Status \| \(in progress\|in review\|planned\)' docs/tasks/*.md` | `README.md` (the format) and this file |
| the leak scan | `leak-scan.sh <worktree>` | prints nothing |

Not in the plan: T-028, which has no task file on `main` or on a pushed branch, and T-052, which
only T-048's release line names.

## Resume point

2026-11-19T17:55:43+00:00: the task files and the plan are pushed. Next: the lead's read.

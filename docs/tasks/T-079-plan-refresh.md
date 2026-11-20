# T-079: The plan says what `main` holds for 0.3.0

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.3.0 (documents only; no version moves) |
| Writer | "Gate 006" |
| Reviewer | the lead reads the documents |
| Depends on | `main` at `de88866` |
| Touches | `docs/plan.md`, this task file |

## Goal

`docs/plan.md` stopped at the state after `v0.2.0`: it called T-037, T-042 to T-047, T-049 to
T-051 and T-053 open or in progress after they merged, did not record `v0.2.1` as released, and had
no row for the tasks from T-055 on. A reader who takes the plan at its word should find in it what
`main` holds and what is in flight for `v0.3.0`.

## Scope

1. Every row of the plan's task table follows from `main`: a task whose branch merged is done and
   names its merge commit from `git log --merges de88866`; a task in flight names its branch and
   the tip read for this task, with no claim about its results; a task file or pushed branch from
   T-055 on gets a row.
2. The Releases table says what the `v0.2.1` tag holds (`git merge-base --is-ancestor <merge>
   v0.2.1`) and what `main` holds for `v0.3.0`.
3. "Open after rc.1": the items that merged say so; the second consumer's backlog lines stay backlog,
   after `v0.3.0`.
4. The task files under `docs/tasks` whose Status disagrees with `main` are listed below, not
   edited: each stays as its writer left it until its writer or the lead closes it.
5. No count or timing that no command produced; the measured figures already in the plan stay as
   they were, with their source.

## Done when

- Every status in the plan names the merge it rests on, or the branch and tip of work not merged.
- The leak scan named in the local rules prints nothing.
- The lead has read the documents.

## Task files whose Status disagrees with `main`

Read from the `Status` row of each file under `docs/tasks` at `de88866`, against the merges on
`main` (`git log --merges de88866`) and the `v0.2.1` tag (`git merge-base --is-ancestor`). Every
other task file says `done` or `merged`, which `main` bears out.

| Task | Status in its file | On `main` |
| --- | --- | --- |
| T-042 | in progress | merged at `8b9208f`, in `v0.2.1` |
| T-043 | in progress | merged at `9b18b32`, in `v0.2.1` |
| T-044 | in progress | merged at `4c09a04`, after the `v0.2.1` tag |
| T-046 | open (Release: the lead sets it) | merged at `389a932`, in `v0.2.1` |
| T-047 | redesigned by the lead after review: validation is never narrowed, only execution | merged at `340cf8b`, in `v0.2.1` |
| T-049 | in progress | merged at `ec66737`, in `v0.2.1` |
| T-050 | in review | merged at `37f3420`, in `v0.2.1` |
| T-051 | in review | merged at `f0d7ca4`, in `v0.2.1` |
| T-053 | in review | merged at `6636e3e`, after the `v0.2.1` tag |
| T-055 | in progress | merged at `7c907b0`, after the `v0.2.1` tag |
| T-056 | in review (the review's fix round green; ...) | merged at `e9e1434`, in `v0.2.1` |
| T-058 | review fixes proven; ready to merge | merged at `cf97401`, in `v0.2.1` |
| T-060 | proven; ready for review | merged at `08668a8`, in `v0.2.1` |
| T-060b | proven; ready for the lead | merged at `a3dd8d5`, in `v0.2.1` |
| T-062 | code review: the review's one fix round is in | merged at `7b525d6`, after the `v0.2.1` tag |
| T-064 | approved for code by the lead (2026-11-19); in progress | merged at `0a3d5a8`, after the `v0.2.1` tag |
| T-065 | the verification's small round green in lane round 310 ... | merged at `7a5088f`, after the `v0.2.1` tag |
| T-066 | design approved by the lead at `c557d16` ...; code after T-056 is green | merged at `cee6491`, after the `v0.2.1` tag |
| T-066b | proven; ready for the lead | merged at `16d9431`, after the `v0.2.1` tag |
| T-067 | design approved by the lead; code written, round to run | merged at `de88866`, after the `v0.2.1` tag |
| T-068 | approved for code (design at `6ebd2b0`); ... | merged at `f611036`, after the `v0.2.1` tag |
| T-070 | in progress | merged at `e7d5a27`, the `v0.2.1` tag |
| T-073 | in progress | merged at `7f62554`, after the `v0.2.1` tag |
| T-074 | done; ready for the lead | merged at `5d7f1b6`, after the `v0.2.1` tag |
| T-075 | in progress | merged at `8b9d40c`, after the `v0.2.1` tag |

## Not in the plan

T-072 and T-077 have no task file on `main` and no branch on `origin`, so they get no row. T-078
has no task file on `main` or on a pushed branch yet; its row says only that it is in progress.

## Proof

- `git log --merges --format='%h %cI %s' de88866`: the merge commits named in the plan.
- `git merge-base --is-ancestor <merge> v0.2.1` for each merge after `v0.2.0`: what the tag holds.
- `git rev-parse v0.2.1^{commit}`: `e7d5a27`; the tag's date from `git tag -l
  --format='%(creatordate:iso-strict)'`: 2026-11-20T08:16:10Z.
- `git rev-parse --short origin/task/<branch>` for each branch in flight, read at 2026-11-20T20:17:08+00:00.

## Resume point

2026-11-20T20:17:08+00:00: `docs/plan.md` and this file written on `task/T-079-plan-refresh` off `main` at
`de88866`; leak scan exit 0. Next: the lead reads the documents; no lane round (documents only).

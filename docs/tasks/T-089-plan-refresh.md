# T-089: The plan says what `main` holds after T-063

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.3.0 (documents only; no version moves) |
| Writer | "Gate 005" |
| Reviewer | the lead reads the documents |
| Depends on | `main` at `2f4fbf0` |
| Touches | `docs/plan.md`, this task file |

## Goal

`docs/plan.md` stopped at T-079 (`4d1f4fe`): it called T-048, T-052, T-057, T-059, T-061, T-063,
T-069, T-071, T-076, T-080 and T-081 in progress after they merged, and had no row for T-071b,
T-077b, T-079b, T-082 to T-084 or T-087, which merged, or for the tasks still open.

## Scope

1. A task merged since T-079 is done and names its merge commit from `git log --merges 2f4fbf0`.
2. A task in flight names its branch and the tip read for this task, with no claim about its
   results: T-072, T-077, T-080b, T-085, T-086, T-088. T-088b has no task file on `main` and no
   branch on `origin`, so its row says only that.
3. The `v0.3.0` row of the Releases table lists what `main` holds after the `v0.2.1` tag and what is
   in progress.
4. No count or timing that no command produced.

## Done when

- Every status in the plan names the merge it rests on, or the branch and tip of work not merged.
- The leak scan named in the local rules prints nothing.
- The lead has read the documents.

## Proof

- `git log --merges --first-parent --format='%h %cI %s' 2f4fbf0`: the merge commits named in the
  plan, `1cbd694` (T-079b) to `2f4fbf0` (T-063); `git log --first-parent --no-merges
  4d1f4fe..2f4fbf0` prints nothing, so no change reached `main` outside a merge.
- `git rev-parse --short origin/task/<branch>` for each branch in flight, read at
  2026-11-21T05:51:08+00:00.
- Titles from each task file's heading, on `main` or on its branch; T-079b's from its merge subject,
  since it has no task file.

## Resume point

2026-11-21T05:51:08+00:00: `docs/plan.md` and this file written on `task/T-089-plan-refresh` off
`main` at `2f4fbf0`. Next: the lead reads the documents; no lane round (documents only).

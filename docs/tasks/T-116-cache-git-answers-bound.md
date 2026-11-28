# T-116: the prune bounds the stored git answers

| Field | Value |
| --- | --- |
| Status | open (backlog; found in T-113, no date asked) |
| Release | a later core release |
| Writer | not assigned |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | T-113 merged (`201e623`) |
| Touches | the Gate cache's git answers and its prune (`crates/gate-host`), their tests, `docs/guide/` where the cache is described, `CHANGELOG.md`, `docs/plan.md`, this task file |

## Goal

Since T-113, a generated CI job prunes its cache with an age bound before each save. A result, a
declaration or a stored git content that no run uses for that long goes. The files under
`git/answers/` do not. A run that asks git anything sets the time of every answer file, used or
not, so no answer ever ages out. An entry restored across many runs carries every answer it ever
stored.

## Required outcome

1. An answer that no run has used within the prune's age bound is removed by that prune. An answer
   a run used stays.
2. A run reads and writes no more git answer files than it does today to keep this working.
3. A removed answer is asked again of git. It is never served wrong.
4. Nothing changes for a cache that is pruned without `--older-than`.

## Proof

- A test fills a cache over runs that ask different git questions. It ages some answers past the
  bound, then shows that the prune removes the unused answers and keeps the used ones. The next run
  gives the same findings.
- All cargo runs go through the lane.

## Resume point

Not started.

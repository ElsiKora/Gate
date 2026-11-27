# T-113: the generated CI cache survives a move of `gate.lock`

| Field | Value |
| --- | --- |
| Status | open (backlog; reported by the first consumer, no date asked) |
| Release | the next core release |
| Writer | not assigned |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `b7a271a` |
| Touches | `crates/gate-host/src/sync/github.rs` (the cache steps of the generated workflow), its tests, `docs/guide/` where the CI cache is described, `CHANGELOG.md`, `docs/plan.md`, this task file |

## Goal

With `[ci.github] cache = true`, every job's restore step has the key
`gate/<os>/<arch>/<hashFiles('gate.lock')>/<job>/<sha>`. Its two restore keys,
`gate/<os>/<arch>/<lock hash>/<job>/` and `gate/<os>/<arch>/<lock hash>/`, both carry the lock hash
(`CACHE_KEY_PREFIX` in `sync/github.rs`). So when any pin in `gate.lock` moves, no key matches and the
job runs cold.

A repository that moves a pin at every release therefore has a cold first `main` run after nearly
every release. GitHub lets a run read only its own branch's caches and the default branch's.

The first consumer measured the static check of one job:

| Run | Time |
| --- | --- |
| cold | 13095 ms, 10166 ms |
| warm | 2733 ms, 3587 ms |

One cold run went over its `--fail-after 12` bound.

## Required outcome

1. After a pin move, a job restores the newest entry of the same runner and job, as long as one
   exists, even when that entry was saved under the previous lock.
2. A restored entry can never be trusted beyond its own keys:
   - a plugin is still verified by sha256 against the current `gate.lock` before it loads;
   - a cached result is used only when its own key matches.
   Show where each is checked. If either holds only by accident, fix that first.
3. The save step stores a new entry whenever a check ran and the restore was not an exact hit, so
   the next run starts warm under the new lock.
4. Nothing else in the generated workflow changes. A configuration without `cache` generates
   exactly what it does today.

A design input from the consumer, not a decision: add a third restore key per job without the lock
hash, `gate/<os>/<arch>/<job>/`, after the two existing ones. The writer weighs it against keying the
lock hash differently and sends the lead the choice before writing code.

## Proof

- Generator tests: the restore keys and the save condition for `cache = true`, and an unchanged
  workflow for `cache = false`.
- A test showing that a cache directory filled under one `gate.lock`, then read under a lock with
  one pin moved:
  - loads only plugins whose sha256 matches the new lock;
  - fetches the moved one;
  - reuses no result whose key changed.
- `bin/gate sync` on this repository writes nothing: it has no `cache`.
- All cargo runs go through the lane.

## Resume point

Not started.

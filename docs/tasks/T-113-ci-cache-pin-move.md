# T-113: the generated CI cache survives a move of `gate.lock`

| Field | Value |
| --- | --- |
| Status | in progress (branch `task/T-113-ci-cache-pin-move`) |
| Release | the next core release |
| Writer | the T-113 writer session |
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

## Design (sent to the lead before the code)

- The consumer's third key, `gate/<os>/<arch>/<job>/`, added after the two keys we had, would match
  nothing: every saved key has the lock hash in the third part. So the job comes before the lock
  hash. Key: `gate/<os>/<arch>/<job>/<lock hash>/<sha>`. Restore keys, in order:
  `gate/<os>/<arch>/<job>/<lock hash>/` (same job, same lock), `gate/<os>/<arch>/<job>/` (same job,
  any lock: the fix), `gate/<os>/<arch>/` (any job, any lock: a new job's first run). The earlier
  fallback, any job under the same lock, can no longer be written as a prefix.
- The save condition is unchanged. `cache-hit` is true only when the primary key matches exactly,
  so a job that restored an entry saved under another lock saves under the current one.
- The places a stale entry is never trusted:
  - `plugins.rs`: `declared()` takes each sha256 from `gate.lock`. `fetch_components()` gets the
    blob through `Cache::get(&pin.sha256)`, which hashes the file again, then hashes the bytes again
    before they load.
  - `usable_declaration()` reads `declarations/<pin sha256>/<core key>`.
  - `results::key()` hashes the core version, the pin's sha256, the request and the grant.
    `results::load()` reads only `results/<pin sha256>/../<key>`.
- The lead accepted all five points of the design (2026-11-27). The lead also asked for:
  - a generator test with two jobs where one id is a prefix of the other (`lint` and `lint-x`);
  - the three trust checks stated on the guide page;
  - the prune step below.
- The lead's decision on outcome 4: "nothing else in the generated workflow changes" now carries one
  more step. With `cache = true` only, each job runs `bin/gate cache prune --yes` right before the
  save, under exactly the save's condition. The prune removes the downloads, declarations and
  results of pins the current `gate.lock` no longer names. A restored entry would otherwise carry
  every earlier pin forward. `modules/` is kept. A cache the prune cannot read fails the step, with
  its message: for example, a cache without `CACHEDIR.TAG`, which `bin/gate` always writes.

## Resume point

The keys, the prune step, their tests and the guide are on `task/T-113-ci-cache-pin-move`.
Next: clippy, then the full lane round, then report the head and each exit code to the lead.

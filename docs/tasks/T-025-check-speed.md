# T-025: A full check is fast cold and the result cache makes a warm run cheap

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | none (from `release/v0.1.0` or main after v0.1.0) |
| Touches | `crates/gate-host/src/run/**`, `crates/gate-host/src/cache/**`, the git answers in `crates/gate-host/src/git.rs`, the history rules in `plugins/git` only if a rule's own work dominates; their tests; architecture §9 |

## Goal

1. A warm `gate check` on an unchanged tree runs no plugin call whose recorded inputs did not change
   (architecture §9). Today the first consumer's tree takes 8.61 s cold and 8.63 s warm with the rc.1
   lock (the lead's conversion report), so the result cache is not hit.
2. A full cold `gate check` with every plugin is no slower than the reference tool on the same tree. The
   second consumer's converted tree takes more than 18 minutes today, against 6.99 s for the reference
   tool (lane ticket 123 has the per-rule split; the lead forwards it).
3. `--timings` names, per rule and per host call kind (`git`, `sync-state`, reads), where the time went,
   so the next regression is found from one run.

## Scope

1. Find why a warm run misses: the key, what is recorded in the read set (a ref-dependent answer, a
   time, a path order), or a cache that is not written. Fix the cause; no new option.
2. From the per-rule measurement, fix what dominates the cold run. If it is history (`sync-state` per
   commit, `tree`, `read-at`), make the answer cheaper or shared across calls and rules; it is cached by
   commit id for good already.
3. Tests:
   - a second run on an unchanged tree runs no plugin call (counted, not timed);
   - changing one read file re-runs exactly the calls that read it;
   - a moved ref re-runs only calls whose read set holds a ref answer;
   - a cold history run makes one `sync-state` per commit, not one per rule or per call.

## Done when

- The tests above, each failing before the change.
- Measured on the first consumer's lock tree (a copy outside this repository):
  cold and warm `gate check`, warm under 1 s; and the lead's second comparison on the second consumer's
  tree, cold no slower than the reference tool. Each number with its command.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the Gate lane on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-18T22:53:08+00:00: from `release/v0.1.0` 3320ce6 on `task/T-025-check-speed`.

- Why warm missed: the result cache was never built. Read sets were recorded, but nothing stored a
  call's result or looked one up (`Cache::results()` had no caller; the cache of the first consumer's
  copy has no `results/` after two runs). With the rc.1 core on that copy, two runs: 7.54 s and
  7.37 s wall, plugins 7071 ms of 7289 ms (`gate check --timings`).
- Built: `cache/results.rs`, a call's findings and read set by (core version, plugin sha256,
  request, grant), reused while every listed prefix, read blob id, change selection, ref answer and
  `sync-state` answer is unchanged. `sync-state` records a digest of its answer, since a fallback
  answer follows the run's configuration. `--timings` prints one line per call (cached, or its time
  and the host's time by kind) and the `sync-state` answers computed and read from the cache.
- `sync-state` per commit per rule: within a run one memo already served every call and rule, but
  two calls running in parallel could both compute the same commit. The memo now computes each
  commit once (a `OnceLock` per commit).
- Not yet: the tests run; the measurement on the copy needs this branch's plugins (the interface
  changed after rc.1, so the rc.1 components do not load); per-rule time needs the plugin to say
  where a rule starts, an interface question for the lead.

# T-025: A full check is fast cold and the result cache makes a warm run cheap

| Field | Value |
| --- | --- |
| Status | planned |
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

2026-11-18: planned; branch `task/T-025-check-speed`.

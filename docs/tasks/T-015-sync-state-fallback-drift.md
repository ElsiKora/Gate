# T-015: No drift from sync-state under the fallback

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 002: plugin form spike" |
| Reviewer | a separate review session |
| Depends on | T-013 (merged) |
| Touches | `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/sync/**`, their tests |

## Goal

At a commit whose `gate.toml` is not a configuration of this core, `sync-state` answers `produced`
from the run's configuration and an empty `drift`. The rule is architecture.md, the bullet
"`sync-state` at a commit".

## Why

T-008's acceptance on main's gate: `git/main-history#direct` reports every direct commit made before a
consumer's move to Gate that changed a generated file together with its source (22 commits on three
trees). Those files were in sync with the tool that wrote them; they differ from this core's
generators only in their header. Content written by another tool cannot be judged by this core's
generators, so under the fallback only paths count.

## Scope

1. Under the fallback (the commit's `gate.toml` does not parse or does not validate, including a
   `gate.lock` mismatch), `drift` is empty; `produced` is unchanged.
2. A generator that fails under the fallback adds its run-output paths to `produced`, not to `drift`.
3. The stored answer's key is unchanged; bump the state version so answers stored before this change
   are not reused.
4. Tests: a fallback commit whose generated file differs from the run's generator output answers it in
   `produced` and not in `drift`; a commit with its own configuration still reports drift; a failing
   generator under the fallback answers its paths in `produced` only.
5. The item 4 timings of T-013 again on the release build (the drift count changes; say by how much).

## Done when

- The tests above, each failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0, run on the pushed commit.

## Resume point

2026-11-17: the change and its tests are pushed on `task/T-015`. Under the fallback `compute` clears
`drift` after `state_in`; `produced` already holds every drifting path and a failing generator's run
outputs. `SYNC_STATE_VERSION` (2) joins the answer key. Tests: the fallback commit (HEAD~3) of
`sync_state_before_the_move_to_gate_uses_the_runs_configuration_and_never_fails_the_call` now answers no
drift while the commit with its own configuration (HEAD~1) still drifts;
`a_generator_failing_under_the_fallback_answers_its_paths_as_produced_only` is new.
The gates (2026-11-17T18:08:37+00:00, `job-all.sh`): fmt, clippy (workspace and
wasm32-wasip2), test (296 passed, 0 failed, 2 ignored), deny, release build and leak scan all exit 0.
T-013 item 4 timings on that release build (`timing.sh`, 1,000 synthetic commits, 200 before the
move to Gate; load average 47.89 before the first run, 42.72 before the second): first run 83.89 s,
second run 0.82 s, 8 drift (was 208 at T-013: the 200 commits from before the move no longer drift),
1,200 stored answers. Next: review.

Merged to main with every gate exit 0 and 374 tests (the merge message records
them). The review found no blocker; its findings are T-014 items 10 to 12.

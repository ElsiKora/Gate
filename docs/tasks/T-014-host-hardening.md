# T-014: Host hardening before v0.1.0

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.1.0 |
| Writer | "Gate 002: plugin form spike" |
| Reviewer | a separate review session |
| Depends on | T-013 (merged) |
| Touches | `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/sync/**`, `crates/gate-host/src/cache/**`, `crates/gate-runtime/src/**`, `crates/gate/src/main.rs`, their tests |

## Goal

The host items the reviews of T-003, T-005 and T-013 kept out of rc.1, so v0.1.0 ships without them.
Nothing here changes a rule's findings.

## Scope

1. **Load failures by kind.** In sync-state, only a `Load` or `Plugin` error marks a component
   unloadable. An environment failure while loading answers for this run only and is not stored.
2. **Validation per plugin.** A component that does not load skips validation only for its own
   plugin; the commit's configuration is still validated against every plugin that loads.
3. **Error kind in `gate sync`.** A fuel, memory or interface failure is reported as a failure of the
   plugin (what the plugin author must fix), not as a configuration error. Which answers are stored
   stays a decision inside sync-state; the user-facing kind matches `gate check`'s.
4. **A test for the CI workflow answering as drifting** when a component of the commit does not load.
5. **An epoch deadline per plugin call and a lower fuel default.** A call that runs past the deadline
   fails as an environment failure and names the plugin and the call. Measure the default against the
   slowest call of the bundled plugins on the synthetic trees and record the margin.
6. **`--rev` without a base.** Take the parent as the base; a root commit is a new branch.
7. **`gate cache prune`** skips tool temporaries younger than one hour.
8. **Generator findings during `gate sync`** surface as `gate/sync#<generator>` findings, with the
   generator's message, and fail the run the way a rule at level error does.
9. **The lock check skips plugins an override covers**, for `check`, `run` and `sync` alike. If T-013
   item 9 already does this for every command, prove it with a test and say so here.
10. **The SDK and WIT docs of `sync-state`** describe the answer as it is: under the fallback `drift` is
    empty; a pinned plugin that does not load does not fail the call, only one that cannot be fetched
    or does not match its pin (`gate-sdk/src/git.rs`, `SyncState::drift` and `Git::sync_state`;
    `wit/gate.wit`, `sync-paths`).
11. **A commit whose component does not load still reports drift:** in
    `a_pinned_component_that_does_not_load_under_this_core_answers_without_failing`, `gen/a.md`
    differs at the commit and the answer holds it in `drift`.
12. **A configuration that validates never falls back.** Loading the configuration refuses what
    `core_outputs` would refuse (`[hooks]`, `[ci.github]`), so `sync-state` under a commit's own
    configuration cannot meet a configuration error there; with a test.
13. A trailing space in the T-015 task file (the lead fixes it in T-015's merge).

Review (one fix round):

- A. The deadline counts the plugin's own time only (architecture, "Time limits count the plugin's
  own work"). Correction: the commit message says the deadline bounds "a host call
  that blocks"; it does not, and must not: time in a host call does not count against it.
- B. `SYNC_STATE_VERSION` 3: items 1, 2 and the lower fuel default change what is stored.
- C. A `Load` from a later call (generate, owns) on a component that has loaded is passing.
- D. The item 9 test adds a second plugin, neither overridden nor pinned: exit 2 naming gate.lock.
- E. The SDK and WIT docs of `sync-state` match the architecture bullet.
- F. A load failure that may pass (a trap or the deadline in `describe`, the machine) at a commit
  fails the `sync-state` call as an environment error; only `Load` or `Plugin` marks a plugin
  unloadable.

## Done when

- A test for each item, failing before the fix and passing after it (item 4 is the test itself).
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0, run on the pushed commit.
- The deadline and fuel numbers for item 5, with the command that produced them.

## Notes

- rc.1 work comes first: run the gates only when no rc.1 job is waiting.
- The plan's "Open after rc.1" list loses each item when it lands here.

## Resume point

2026-11-17: every item is written and pushed on `task/T-014`; nothing is compiled yet, so the first
run of the gates on the pushed commit is the first build of items 1–5 and 8.

- Items 6, 7, 9. Item 9: T-013 item 9 already skips the overridden plugins in `check`
  and `sync`; `gate run` loads no plugin and takes no `--plugin`, so it never checks a plugin pin.
  The test proves all three commands.
- Items 1–4. Item 1 is covered by the unit test of the two failure classifications
  (`plugins.rs`, `only_a_load_or_interface_failure_marks_a_component_unloadable_for_good`) and by
  the end-to-end test that a `Load` failure's answer is stored; an environment failure while
  loading cannot be provoked end to end.
- Item 8: `gate sync` and `gate sync --check` report generator findings as
  `gate/sync#<plugin>/<generator>` at level error and exit 1; the probe's `write` generator has a
  `report` option for the test.
- Item 5: engine epoch every 10 ms, a 60 s deadline per call (600 s since the deadline
  decision at the end of this file; `ErrorKind::Deadline`,
  naming the plugin and the call), fuel default 10^11 (was 10^12). Measured
  2026-11-17T18:01:00+00:00 with
  `cargo test --release --locked -j 4 -p gate-runtime --test measure -- --ignored --nocapture`:
  fuel to within 1% (bisection on the fuel limit), time the fastest of three under the default
  limits, on a machine shared with other builds.

  | Call | Tree | Fuel | Under 10^11 | Time | Under 60 s |
  | --- | --- | --- | --- | --- | --- |
  | `typescript/parse` | 100 files × 300 lines, 0.7 MB | 1.33e8 | 752× | 0.025 s | 2359× |
  | `hello/marker` | 100 files × 300 lines, 0.7 MB | 4.08e7 | 2454× | 0.003 s | 22017× |
  | `typescript/parse` | 2,000 files × 300 lines, 14.6 MB | 2.64e9 | 37.9× | 0.583 s | 103× |
  | `hello/marker` | 2,000 files × 300 lines, 14.6 MB | 8.16e8 | 122× | 0.064 s | 943× |
  | `typescript/parse` | 200 files × 5,000 lines, 24.2 MB | 4.29e9 | 23.3× | 1.523 s | 39.4× |
  | `hello/marker` | 200 files × 5,000 lines, 24.2 MB | 1.34e9 | 74.9× | 0.081 s | 737× |

  The slowest call, `typescript/parse` over 24.2 MB, leaves a margin of 23× in fuel and 39× in
  time (the last column is against the 60 s deadline of that round).
- The plan's "Open after rc.1" list no longer holds the items of this task.

The first gates: fmt, test, deny, release build, leak scan and the measurement exit
0; clippy 101 on five `unused_qualifications` in `plugins.rs` and one in the tools test (fixed in
the next commit; clippy stopped at gate-host, so the gate crate and gate-host's tests are not yet
clippy-checked). A second round (2026-11-17T18:11:59+00:00) did not compile: T-015's
round had built gate-runtime from main into the shared target directory after this checkout's files
were written, so cargo took its artifacts as current. The round scripts now touch every `.rs` file
after checkout.

Items 10–13 from T-015's review (2026-11-17): `task/T-015` is merged into this branch. Item 10: the
SDK and WIT docs. Item 11: the test's `gen/a.md` is edited by hand at the commit and the answer
expects `drift gen/a.md`. Item 12: `Workspace::from_snapshot` ends with
`sync::check_core_outputs`, which runs `core_outputs` with no generator and no history rule (which
jobs read history changes a job's checkout only); test
`a_configuration_whose_hooks_or_workflow_sync_refuses_does_not_load` in `cli.rs` (`check`, `list`
and `sync --check` exit 2 naming the section). Item 13: the lead's. Round (2026-11-17T18:30:23+00:00): fmt, wasm clippy, deny,
release build, leak scan exit 0; test 304 passed, 1 failed, 3 ignored; clippy 101 on
`Duration::from_secs(2 * 3600)` in the tools test. The failure: with T-015 merged the fallback
answers no drift, so the item 2 test could no longer tell the fallback from the commit's own
configuration by drift; the run's configuration now writes `gen/b.md`, and the fallback answers
`produced gen/a.md,gen/b.md` where the commit's own would answer `gen/a.md` alone.

Green round (2026-11-17T18:34:36+00:00, `job-all.sh`): fmt, clippy
(workspace, all targets, and wasm32-wasip2), test (305 passed, 0 failed, 3 ignored), deny, release
build and leak scan all exit 0.

Fix round for the review (2026-11-17), with main merged:

- A: `State` sums the time of every host call (`git.*`, `tree.read`) and the epoch-deadline
  callback traps only when the wall time less that sum reaches the deadline, else moves the
  deadline by what is left. Tests: `time_in_a_host_call_does_not_count_against_the_deadline`
  (`git.head` sleeps 5 s under a 2 s deadline and the call succeeds) and the existing
  `a_call_past_the_deadline_fails_naming_the_plugin_and_the_call` (guest work alone).
- B: `SYNC_STATE_VERSION` 3.
- C: `passing_call_failure` holds `Load`; the classification unit test moves it.
- D: in `the_lock_check_skips_a_plugin_an_override_covers_in_check_run_and_sync`.
- E: `gate-sdk/src/git.rs` (`Git::sync_state`) and `wit/gate.wit` (`sync-state`); docs only.
- F: `commit_plugins` returns an environment error for a load failure that is not lasting, and
  `CommitPlugins` no longer carries `passing`. The probe gains a `describe-traps` feature that builds
  a plugin `other` whose options schema panics in `describe`; test
  `a_component_whose_describe_fails_in_a_way_that_may_pass_fails_the_call` (exit 2, environment
  error, nothing stored).

Round (2026-11-17T18:58:40+00:00): fmt, wasm clippy, test (385 passed, 0 failed,
3 ignored), deny, release build and leak scan exit 0; clippy 101 on four `unused_qualifications`
(`Duration` in `state.rs`, newly imported) and one unchecked `Duration` subtraction in the deadline
callback, fixed by `saturating_sub`.

Green round (2026-11-17T19:03:28+00:00, `job-all.sh`): fmt, clippy
(workspace, all targets, and wasm32-wasip2), test (385 passed, 0 failed, 3 ignored; the four new or
changed tests of A, D and F pass), deny, release build and leak scan all exit 0.

Correction to F (lead, 2026-11-17): running out of fuel or hitting the memory limit in `describe` is
lasting, since the limits are the core's and `describe` has no input. `lasting_load_failure` now holds
`OutOfFuel` and `OutOfMemory`, so such a plugin is marked unloadable, the commit answers, and the
answer is stored. Only a failure that may pass (a trap, the deadline, the machine) fails the call.
Tests:
- `a_describe_that_never_ends_fails_to_load_out_of_fuel_on_every_run` (gate-runtime): the probe's
  new `describe-spins` feature builds a plugin `other` whose options schema never ends; under 5·10^7
  fuel, `load` fails with `OutOfFuel` twice.
- `only_a_failure_that_recurs_marks_a_component_unloadable_for_good` (gate-host): the
  classification, `OutOfFuel` and `OutOfMemory` among the lasting kinds.
- `a_pinned_component_that_does_not_load_under_this_core_answers_without_failing`: the end-to-end
  path of a lasting load failure (answers with drift, stored).

Deadline decision (lead, 2026-11-17, architecture "Time limits"): fuel decides and the deadline is
only a backstop, so the default deadline is 600 s (was 60 s). The arithmetic, from the measurement
above (`cargo test --release --locked -j 4 -p gate-runtime --test measure -- --ignored
--nocapture`): the slowest call ran 4.29·10^9 fuel in 1.523 s, about 2.8·10^9 fuel per
second, so the default 10^11 fuel is about 35.5 s of guest time. On a machine ten times slower that
is about 355 s, under 600 s by a factor of 1.7, so fuel runs out first and the failure is the lasting
`OutOfFuel`, not the `Deadline` that may pass. The core has no setting that lowers the fuel for one
run, so there is no end-to-end `gate check` whose `describe` runs out of the default fuel; the three
tests above cover the chain. The deadline test keeps its 2 s deadline set through the runtime's
`Config`.

Green round (2026-11-17T19:18:23+00:00, `job-all.sh`): fmt, clippy
(workspace, all targets, and wasm32-wasip2), test (386 passed, 0 failed, 3 ignored), deny, release
build and leak scan all exit 0. Next: merge after the rc.1 tag (the lead's).

Green round (main merged, default deadline 600 s; 2026-11-17T19:28:45+00:00,
`job-all.sh`): fmt, clippy (workspace, all targets, and wasm32-wasip2), test (470
passed, 0 failed, 3 ignored; the count includes main's new tests), deny, release build and leak scan
all exit 0. Next: merge after the rc.1 tag (the lead's).

# T-077b: `gate/baseline.json` is replaced whole, and its writers take turns

| Field | Value |
| --- | --- |
| Status | reviewed (one fix round); lane round 420 green; ready to merge |
| Release | v0.3.0 |
| Writer | Gate 001 |
| Reviewer | a session that did not write it |
| Depends on | `main` at `20480703` (T-076's `files::replace` and `lock::Guard`) |
| Touches | `crates/gate-host/src/lib.rs`, `crates/gate-host/src/baseline.rs`, `crates/gate-host/src/lock/mod.rs`, `crates/gate-host/src/files.rs`, `crates/gate/tests/plugins.rs`, `docs/guide/configuration.md`, `CHANGELOG.md` |

## Goal

T-077's design found this, outside its own scope. `gate baseline write` and `gate baseline prune`
write `gate/baseline.json` with `fs::write` (`lib.rs:277`, `lib.rs:355`), which truncates the file
and then writes it, and nothing orders two runs:

- A `gate check` that reads the file meanwhile can see it empty or cut short, and fails with a
  configuration error.
- `gate baseline prune` works from the baseline it read when its workspace opened, before its
  full check. A `gate baseline write` or another prune that writes in the meantime is undone when
  prune writes. An entry the other run wrote is lost, or one it removed comes back. For example,
  prune A removes X, prune B (which read the file before A wrote it) removes Y, and B's write puts
  X back.

## What each command reads and writes

| Command | Reads | Writes |
| --- | --- | --- |
| `gate baseline write` | the tree, through a full check; not the baseline | `gate/baseline.json`, replaced with every finding |
| `gate baseline prune` | the tree, through a full check (which does not use the baseline), and the baseline, loaded when the workspace opens | `gate/baseline.json`, with the stale entries removed and the unkeyed ones re-keyed, only when something changed |
| `gate check` and every other reader | the baseline, when the workspace opens | nothing |

## The change

1. **Replaced whole.** Both writers write through T-076's `files::replace`: a synced temporary
   file beside it, renamed over it, the directory synced. A reader sees the old file or the new
   one, never part of one.
2. **A guard for the baseline.** `lock::Guard::take_baseline(&Git)` is T-076's guard on a second
   file in the worktree's git directory, `gate.baseline.guard`. While another run holds it, it
   prints `gate: waiting for another gate run to finish writing gate/baseline.json (<path>)`. If it
   cannot be taken, it fails as an environment error with T-076's advice. It is the same mechanism
   on a file of its own, not `gate.lock.guard`. The waiting line and the error should name the file
   being written. Sharing the file would also make a `gate lock --set` wait for a baseline write,
   and the reverse, with nothing to gain.
3. **Read under the guard.** Each writer runs its full check without the guard, since the check
   reads no baseline, and then takes the guard. `baseline prune` reads `gate/baseline.json` from the
   working tree again under the guard, prunes that against the check's findings, writes it, and
   lets go. So a run that wrote meanwhile is the starting point, not something to overwrite.
   `baseline write` replaces the file under the guard. A prune that waits for a write prunes what
   the write wrote, and a write that waits for a prune replaces what the prune wrote. The guard is
   held for one read and one write, not for the check.
4. **Only what the check judged.** Under the guard, prune removes or re-keys only the entries
   that the baseline held when its workspace opened, before its check
   (`Baseline::pruned_since`). An entry another run wrote meanwhile may come from a tree the
   check never saw, so it is kept as it is. Without this, a prune whose check saw `x` while a
   write added `z` and wrote `x`, `z` would remove `z`, and no serial order gives that result.
5. **Untracked, checked again.** The untracked check runs again on the file read under the
   guard. A `gate/baseline.json` that did not exist at check time but exists, untracked, under the
   guard stops prune with the same error, as the guide says.
6. **Errors say what to do.** A baseline that cannot be read under the guard, a directory that
   cannot be created, and a replace that fails (`files::replace`, also used for `gate.lock` and
   the files `gate sync` writes) now each name the next step.
7. The baseline loaded when the workspace opened still decides what `gate check` reports. Prune
   uses it only to fail early on a file edited by hand (configuration error, exit 2, nothing
   written), as today. The re-read under the guard is checked the same way.

## Tests

1. `a_baseline_prune_waits_for_a_baseline_written_meanwhile_and_prunes_that` (fails today), in
   `crates/gate/tests/plugins.rs`, uses the probe plugin as `baseline_prune_removes_only_stale_entries_and_never_adds_one` does:
   - The committed baseline accepts `x` and `y`. The tree is then changed to report `x` and `z`.
     The content a `gate baseline write` gives for it (`x`, `z`) is recorded, and the committed
     baseline is put back.
   - The test takes `gate.baseline.guard`, as a first run would, and starts `gate baseline prune`.
     It reads the run's stderr until the waiting line or its end.
   - It writes the recorded `x`, `z` baseline, as the first run's `gate baseline write` would, then
     lets go.
   - Expected: prune waited; it says `removed 0` (nothing in `x`, `z` is stale) and exits 0, and
     the file is the `x`, `z` baseline.
   - Today prune does not wait. It prunes the baseline it read before (`x`, `y`) and says
     `removed 1`, so the test fails on "prune waited" and on its output. In a real overlap, that
     write would undo the other run's entry for `z`.
   - Order comes from the guard and the run's own output only, as in T-076.
2. `a_baseline_prune_keeps_an_entry_written_meanwhile_from_a_tree_its_check_did_not_see`: the
   committed baseline is `x`, `y`; the prune's tree reports only `x`. While prune waits, `z` is
   added and the first run writes `x`, `z`. Expected: `removed 0 ... 2 kept`, and the file is
   `x`, `z`. Without `pruned_since`, prune removes `z`.
3. `a_baseline_that_appears_untracked_while_prune_checks_stops_it`: there is no baseline when
   prune starts. An untracked one is written while prune waits. Expected: exit 2 with the
   `git add` advice.
4. **Mutation steps** in the lane round: the guard removed from `prune_baseline` (test 1 must
   fail), and `pruned_since` replaced by `pruned` (test 2 must fail).

## Done when

- Test 1 passes, and fails under the mutation step.
- The existing baseline tests pass unchanged, including the one for a file edited by hand, which
  is still exit 2 with nothing written.
- One lane round on the pushed tip, on this task's own target directory: fmt, clippy, test, deny
  and the release build exit 0, and the log shows `gate-host` and `gate` compiled from this task's
  worktree.
- The leak scan prints nothing.

## Resume point

2026-11-21T03:57:57+00:00: task note written on `task/T-077b-baseline-atomic` from `20480703`.
2026-11-21T04:05:17+00:00: code and test 1 in `80d980cb`. Independent review: nothing blocks, with
one fix round: prune judges only entries its check saw (`pruned_since`, test 2), the untracked
check is repeated under the guard (test 3), the read, create and replace errors name the next step,
and this note is current. Round 419 on `80d980cb` was withdrawn before it started (by its process
group). Next: one lane round on the fixed tip, then the merge.
2026-11-21T05:47:11+00:00: lane round 420 on `bd8896bb`: fmt, clippy, test, deny and release exit
0; `gate-host` and `gate` compiled from this task's worktree; the three tests above pass; the guard
mutation fails test 1 (exit 101), the `pruned_since` mutation fails test 2 (exit 101); the leak
scan prints nothing. Next: the merge.

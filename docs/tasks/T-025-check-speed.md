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
- Measured on the copy (this branch's core and plugins through `--plugin`, gate check --timings, a
  machine at load average 110 to 185, so only the shape counts): cold 20.75 s wall on an empty cache;
  warm 0 of 12 calls run, 1.5 to 2.8 s wall, of which loading the seven plugins took 0.57 to 2.05 s
  and checking the stored results 0.26 to 0.69 s. Plugins now load on a thread each; `--timings`
  prints `load` apart from `plugins`.
- 2026-11-18T23:36:13+00:00: merged release/v0.1.0 c32848a (version 0.1.0, fixtures pin the core under
  test). Local: `cargo test --locked -p gate-runtime -p gate-host` exit 0; `cargo test --locked -p
  gate --test plugins` 40 passed, 4 failed (the version fixtures, before the merge); fmt and clippy of
  the three crates exit 0. Lane ticket 143 queued: the five gates, the four new tests on c32848a (each
  must fail), then one cold and three warm runs on the copy.
- Second goal, git first. Copy of the second consumer's converted tree (10450 tracked files), this
  branch's core, `gate check --timings --rule 'git/*'`, a fresh cache, load average 150 to 200, so
  wall times only give the shape:
  - 1674c77 and before: the old main-history over 100 first-parent commits did not finish in 300 s;
    the whole history (1013 first-parent commits since activation) did not finish in 300 s.
  - Cause: `sync-state` for every direct commit, about 4 s each here. Of the 778 non-merge
    first-parent commits since activation (`git rev-list --first-parent --no-merges`), 196 are
    releases, 570 direct commits change documentation only, 12 change a code path.
  - 68cc1d8: main-history asks `sync-state` only for a direct commit that changes a path outside
    `documentation`, and only for those paths. Whole history: 179.95 s wall, 21.74 s user; host time
    by kind: changed-paths 51.2 s in 778, sync-state 50.8 s in 12, merge-tree 20.7 s in 235,
    is-ancestor 18.7 s in 237, tree 17.3 s in 234, read-at 7.7 s in 810. A documentation file that
    differs from `gate sync` at a direct commit is no longer reported by main-history (the lead's
    direction; the reference asks a direct commit only for its changed paths).
  - Then: `tree`, `changed-paths` (a diff of trees read object by object), `read-at` and
    `is-ancestor` (a commit-graph walk by generation) through one `git cat-file --batch` reader per
    run. Whole history: 258.02 s wall (with tests running beside it), 12.21 s user.
- Lane ticket 143 on 68cc1d8: fmt, clippy, test (892 passed, 0 failed), deny and release exit 0;
  the four new tests each exit 101 on c32848a. Its cold and warm runs on the first consumer's copy
  exited 2: the copy still pinned 0.1.0-rc.1 (moved to 0.1.0 since).
- c4fa527, 1bf05dd: the reader diffs trees one level per round trip, keeps trees for the run, and
  loads the ancestry with one `git rev-list --parents`. The reference's calls as single git
  processes on the second copy at load about 150: 130 s wall, 37 s user and sys.
- d198ed0: answers about full commit ids (tree, changed-paths, merge-tree, is-ancestor) are stored
  in `git/answers/`, so a history call that reruns because main moved asks git only about the new
  commits; `--timings` prints the count. Lane ticket 147 (per plugin cold and warm on both copies,
  main moved by one commit, the reference's calls) and 148 (the five gates) are queued on d198ed0.
- Found: the branch's nestjs plugin fails the second copy's check, "hit the fuel limit of
  100000000000 units" (the default since bfd86c4, on main and release/v0.1.0); sent to the lead.
  Each nestjs rule builds its own TypeScript instance and parses its files again: alone, the
  costly rules take 0.6 to 2.5 s CPU each (`/usr/bin/time`, one `--rule nestjs/<r>` per run). A
  parse shared by a call's rules is the fix, in `plugins/nestjs` and the TypeScript library, which
  this task's touch list leaves out: put to the lead.
- Wall times on this machine are about five times CPU time (18 cores, load average about 100);
  compare CPU.
- 2026-11-19T01:41:53+00:00: rebased onto `fix/fuel-default` 5b5ca2c (fuel 10^12, `SYNC_STATE_VERSION` 4);
  the tree differs from the old tip by that branch's changes only. Lane tickets 147 and 148 withdrawn
  and requeued behind the fuel fix as 151 (measure) and 152 (the five gates). The lead moved
  nestjs's own cost to T-030; this task keeps the host. The nestjs rules one at a time are in the
  shared notes for T-030.
- `merge-tree` of two full ids goes through one `git merge-tree --stdin` per run; when that fails and
  a git process of its own answers the question, the run stops using it. Found while testing it:
  `git merge-tree` exits 1, printing nothing, for an argument that names no commit, which the host
  answered as a conflict and stored; it is an error now.
- 2026-11-19T05:00:28+00:00: review fix round on 53f15b8. An is-ancestor or merge-tree answer is stored
  only when git read both commits and the clone is not shallow; git's "not an ancestor" for a commit
  the clone lacks is answered but not stored. A `git cat-file` reader that stopped is an error; a
  failed batch read kills the child before joining its writer. A cache write that fails is one
  warning, and the check still reports. A working-tree file's id hashes the bytes a plugin reads
  (`hash-object --no-filters` for paths that git converts). Tests: a missing commit then a fetch,
  merge-tree per git version and never from a shallow clone, an unwritable cache, a line-ending edit
  under `core.autocrlf`, and changed rule options, plugin options or plugin bytes rerunning the call.
  Each fails with its fix (or, for the last, its part of the result key) taken out. The is-ancestor
  fix re-asked a recorded yes too, so a warm run read a stored answer per call: a stored result now
  asks again only what can change (`GitQuery::lasting`). Not fixed:
  `read-at` of a commit the clone lacks is still stored as absent.
- Open: per-rule time and fuel need the plugin to say where a rule starts (an interface question,
  put to the lead); the 12 `sync-state` answers at about 4 s each.

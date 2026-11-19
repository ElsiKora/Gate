# T-025: A full check is fast cold and the result cache makes a warm run cheap

| Field | Value |
| --- | --- |
| Status | merged for `v0.2.0`; the cold bound is open, in T-047 |
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
   tool (the lead forwards the per-rule split).
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
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19T17:53:44+00:00: merged into `main` twice and released in `v0.2.0`: at `264264f` (branch
tip `15fac7c`, the result cache, batched git objects, stored git answers, fuel in `--timings`) and
at `b13c5bd` (branch `task/T-025-check-speed-v0.2.0`, tip `f93bd4f`, the rest of the speed-up). Goal
2 is not met: a full cold check is not yet shown to be no slower than the reference tool. What holds
a cold run over that bound is the core compiling every configured plugin, which moves to T-047
(T-037's measurement on its branch). The other open items of the last entry under "Earlier" are not assigned to a
task yet: per-rule time and fuel need the plugin to say where a rule starts, the `sync-state`
answers at about 4 s each, `gate cache prune` never removing `git/blobs/`, and the blob contents a
run reads staying in memory for the whole run.

Earlier:

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
- 2026-11-19T06:19:00+00:00: the warm check of the git call. A profile of a warm `--rule 'git/*'` run on the
  second copy (macOS `sample`, 1 ms, release with debug symbols): checking the call's stored result
  asks `sync-state` again, and all 12 stored answers follow the run's configuration, so finding
  each one needed `run_key`: 6921 of 9592 samples, 6181 of them running every generator on the
  run's tree, 740 loading every plugin a second time; `state_key` hashed the 17.5 MB of `--plugin`
  builds per key (2645). The CPU of the whole run was 0.85 s user and 0.83 s sys
  (`/usr/bin/time`) at load average about 270, so its 15 to 22 s wall time is the machine's load.
  992fad9 hashes the builds once per run; 40308c8 stores a generator's output with its read set in
  the result cache (format 2), so the run's generators, `sync-state` at a commit and `gate sync` run
  only generators whose read files changed; 7ac0379 gives the history the run's own loaded plugins.
  At 7ac0379 (profile build, `--rule 'git/*'`, a copy of the measured cache, one run to store the
  format-2 entries first): warm 2.12 s wall, 0.28 s user, 0.46 s sys at load average 124; the call
  "cached, checked in 570 ms", generators "0 ran, 4 from the cache". On 943fea1 (ticket 151) the
  same run took 5.73 s wall, 0.77 s user, 0.53 s sys, plugins 4805 ms. What is left of a warm run is
  loading the plugins (`Runtime::new`, compiled modules read back). The cold run's largest host
  share is `read-at`: 19.2 s in 810, the next item.
- 2026-11-19T06:57:07+00:00: 34ab5b5 stores `read-at` answers (the blob id) and each blob's content once.
  Main moved by one empty commit on the second copy, `--rule 'git/*'`, profile build, load about
  150: the first such run asked git all 810 (`git.read-at 26518 ms in 810`, call ran 46678 ms,
  "812 asked, 1482 from the cache"); main moved once more, the call ran 17068 ms with
  `git.read-at 632 ms in 810` and "2 asked, 2293 from the cache"; 3.40 s user, 2.20 s sys. The
  call's own work is fuel 2.73e10. Both runs spent 12 to 14 s in the snapshot, because a copied
  tree's git index no longer matches its files (a clone in place does not pay it).
- 2026-11-19T07:24:08+00:00: the cold `sync-state` cost. A profile of a cold git-only run at 34ab5b5 (12
  answers computed, 47 generators ran, 5 from the cache): 78.3 s wall, 6.9 s user, 2.5 s sys for the
  whole run; of the samples, `sync_state_at` 49111, its generators 34736 (about 14000 of them
  `cat-file` reads), `Snapshot::open` 8564. 343f163 lets the commit snapshots of one run share their
  tree listings, blob contents and one `cat-file` reader. Not measured yet.
- Review of 15fac7c..e25a2ec, fixed: a `read-at` miss is stored as absent only when the commit's
  tree names no file at the path (`ls-tree`); a partial clone that cannot fetch the file fails the
  call and stores nothing. A stored blob id whose content neither the cache nor git has fails the
  call and says to fetch or `gate cache prune`, never "no file". A git answers file whose entry has
  the wrong shape for its kind is skipped. Tests for the generator cache: other options, another
  build, a new file under a listed directory, a missing file created, `gate sync --check` answered
  from the stored outputs.
- Open: per-rule time and fuel need the plugin to say where a rule starts (an interface question,
  put to the lead); the 12 `sync-state` answers at about 4 s each.
- Follow-ups: `gate cache prune` never removes `git/blobs/`; the blob contents a run reads stay in
  memory for the whole run.

# T-056: `gate cache prune` bounds the whole cache

| Field | Value |
| --- | --- |
| Status | in review (the review's fix round green; design approved by the lead, 2026-11-19) |
| Release | v0.2.1 |
| Writer | "Gate 004: public presentation" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `f0d7ca4` |
| Touches | `crates/gate-host/src/cache/` (`mod.rs`, `results.rs`, `answers.rs`), `crates/gate-host/src/plugins.rs` (the result, blob and `sync-state` reads), `crates/gate/src/main.rs` and `crates/gate/src/distribution.rs` (`gate cache prune`), their tests, `README.md` (the command table), `docs/architecture.md` (Cache and speed), `CHANGELOG.md` |

## Goal

`gate cache prune` keeps a machine's cache bounded without breaking a check. Since rc.1 it removes
only downloads no lock pins (`blobs/sha256/`, `core/`, `tools/`), `modules/` when no plugin is
pinned, and stale files in `tmp/`. It never removes `results/` or anything under `git/`
(`git/answers/`, `git/blobs/`, `git/sync-state/`), so these grow for good, and it has no
`--older-than` or `--max-size`, which the architecture names (T-025's follow-ups, plan "Open after
rc.1"). On the writer's own cache on 2026-11-19 (`du -sh`, `find -type f | wc -l`):
`git/sync-state/` 31M in 7998 files, `modules/` 171M in 127, `blobs/` 54M in 16, `core/` 48M in 4,
`results/` 796K in 18, `git/answers/` 12K in 3.

## Design

The cache holds two kinds of entry. **Downloads** (`blobs/sha256/`, `core/`, `tools/`) are pinned:
an entry stays exactly while a `gate.lock` (or a `[tools]` sha256) of a worktree of the named
repositories names it, as today, and no bound ever removes a pinned one. **Derived entries** are
results, `sync-state` answers, answer files and stored blob contents: each is recomputed on a miss,
so removing one costs time and never a wrong answer. Without a bound, `prune` also removes the
derived entries nothing can use: the results of a plugin no lock pins (results are laid out by
plugin, `results/<plugin sha256>/<2 hex>/<key>`, format 3, so they go with the plugin as a download
does, and the format-2 layout, `results/<2 hex>/`, goes too, as an older results format), and each
stored blob content that no stored `read-at` answer names (`prune` reads `git/answers/` for the
ids), once it was stored more than a day ago, since a run stores a content before the answer file
that names it. An answer file `prune` cannot read (other than one a run removed meanwhile) stops it
with an error before anything is removed: the contents it names are unknown. A derived entry's last use
is its modification time, which a run sets to now each time it uses the entry (a result hit, a
`sync-state` hit, a blob content read, the answer files it loads). `--older-than <days>` removes the
derived entries last used more than that many days ago. `--max-size <size>` (`512M`, `2G`: MiB, GiB)
then removes derived entries, the least recently used first and by path on a tie, until the whole
cache, downloads included, is within the size; when the pinned downloads and `modules/` alone exceed
it, `prune` says so with their size and removes no pinned entry. `modules/` stays as it is: removed
as one unit when no plugin is pinned, and counted in the size, but never entered, because wasmtime
bounds it itself (its defaults: 512 MiB and 65536 files, cleaned hourly, to 70%). A concurrent check
stays safe: every entry is written to `tmp/` and renamed into place, and `tmp/` keeps files younger
than a day; a removed file that a check has open stays readable to it; a check that finds an entry
gone recomputes it (results, `sync-state`) or asks git (blob contents): a pruned blob content is
read from git's own object by its stored id, as a full clone already did. Where git lacks the object
too (a partial clone that cannot fetch it), the stored `read-at` answer is asked of git again as if
it were not stored; that fails as well, and the error says to run `git fetch` and no longer suggests
`gate cache prune`, which did not help. A removal that empties a `<2 hex>/` directory (or a
plugin's results directory) removes it too; a run that finds its directory gone at the rename
creates it again. Accepted as designed: the merged answer file is touched on every run, so in an
active cache only `--max-size` trims it.

## Scope

1. The design above, in `crates/gate-host/src/cache/` and the readers in `plugins.rs`.
2. `gate cache prune [<repository>...] [--older-than <days>] [--max-size <size>] [--yes]`: lists
   by default, removes with `--yes`, as today; each listed entry says why it goes (unpinned,
   unused, older than, over the size).
3. The README's command table and the architecture's "Cache and speed" say what the command does.
4. Not in scope: a bound inside `modules/`, a `[cache]` table in `gate.toml` (the cache belongs to
   the machine, not to one repository), a prune a run starts by itself.

## Done when

Tests, each failing before the change:

- A pinned download stays under `--older-than 0` and under `--max-size` smaller than the downloads.
- Results of an unpinned plugin go; of a pinned plugin they stay without a bound and go when older
  than `--older-than`.
- A stored blob content no answer names goes; one an answer names stays without a bound.
- `--max-size` removes the least recently used derived entries first and stops within the size;
  over the size with only pinned entries left, it says so and removes nothing pinned.
- A hit sets the entry's modification time, so a used entry survives `--older-than`.
- A check after its blob contents were removed, in a repository whose git lacks the blobs too, asks
  again and fails with advice to run `git fetch`, never `gate cache prune`; before the change it named
  `gate cache prune`.
- A stored content no answer names stays while younger than a day; an answer file `prune` cannot
  read stops it, and nothing is removed.
- Each use that sets a last use (an answer file loaded, a content read, a `sync-state` read, a
  result hit) keeps its entry under `--older-than`.
- An emptied `<2 hex>/` directory goes with its last file.
- A malformed `--older-than` or `--max-size` is a configuration error, exit 2, that names the form.

The lane: fmt, clippy, the tests above, deny, the release build, and the leak scan, on the pushed
commit; `rustfmt --check` natively before each commit.

## Proof

### The review's fix round

Lane ticket 265 on `27ca81d` (2026-11-20T00:25:09Z to 00:41:02Z, the lane's exit 0), the same steps:

| Step | Command | Exit |
| --- | --- | --- |
| format | `cargo fmt --all --check` | 0 |
| lint | `cargo clippy --offline --locked --workspace --all-targets -j 4 -- -D warnings` | 0 |
| tests | `cargo test --offline --locked --workspace -j 4 --no-fail-fast`: 1042 passed, 0 failed | 0 |
| licences | `cargo deny --offline --locked check` | 0 |
| release build | `cargo build --release --offline --locked -j 4` | 0 |

Then each change taken out, one at a time, with its tests (`--lib` for a unit test); every one of the
14 is caught by a failing test, 16 runs in all, each exit 101:

| Change taken out | Tests that fail |
| --- | --- |
| results of an unpinned plugin go | host `prune_removes_derived_entries_...` |
| blob contents no answer names go | host `prune_removes_derived_entries_...` |
| an unnamed content is kept a day | host `prune_removes_derived_entries_...` |
| an unreadable answer file stops prune | host `an_answer_file_prune_cannot_read_stops_it_before_anything_is_removed` |
| `--older-than` | host `prune_removes_derived_entries_...`; gate `a_used_result_survives_...` |
| least recently used first, by path on a tie | host `max_size_removes_the_least_recently_used_...` |
| the pinned-alone-exceed message | host `prune_removes_derived_entries_...`; gate `cache_prune_lists_then_removes_only_what_no_lock_pins` |
| an older results format has its own reason | host `prune_removes_derived_entries_...` |
| emptied `<2 hex>/` directories go | host `prune_removes_derived_entries_...` |
| a result hit sets the last use | gate `a_used_result_survives_...` |
| loading an answer file sets its last use | lib `cache::answers::tests::the_answer_files_a_run_loads_...` |
| reading a stored content sets its last use | lib `cache::answers::tests::the_answer_files_a_run_loads_...` |
| reading a `sync-state` sets its last use | lib `plugins::tests::a_sync_state_a_run_reads_counts_as_used` |
| a stored `read-at` answer without content is asked again | gate `a_stored_file_whose_content_...` |

The round before it (ticket 259 on `9a01fb5`) stopped at clippy: `cloned_ref_to_slice_refs` on two
one-element slices in new tests; `27ca81d` writes them with `std::slice::from_ref`. After ticket 265,
`main` moved to `a3dd8d5` (T-058, T-060, T-060b), and the branch is rebased onto it: one conflict, the
`OnceLock` import of `crates/gate/tests/plugins.rs`, which `main` no longer uses there.

On the rebased tip, lane ticket 277 on `1b141d3` (2026-11-20T01:33:03Z to 01:56:04Z, the lane's exit 0):
fmt, clippy, deny and the release build exit 0; the workspace tests exit 0 with 1044 passed and 0
failed; the same 14 changes are each caught by a failing test, 16 runs, each exit 101.

### The first full round

Lane ticket 252 on `3d8c4a4` (2026-11-19T22:32:01Z to 22:50:18Z, the lane's exit 0), the script's
steps in order:

| Step | Command | Exit |
| --- | --- | --- |
| format | `cargo fmt --all --check` | 0 |
| lint | `cargo clippy --offline --locked --workspace --all-targets -j 4 -- -D warnings` | 0 |
| tests | `cargo test --offline --locked --workspace -j 4 --no-fail-fast`: 1016 passed, 0 failed | 0 |
| licences | `cargo deny --offline --locked check` | 0 |
| release build | `cargo build --release --offline --locked -j 4` | 0 |

Then each change taken out of the same commit, one at a time, with the tests named for it; a change
counts as caught only when a named test fails (a build error would not count):

| Change taken out | Test (`cargo test -p <crate> --test <target> -- <name>`) | Exit, result |
| --- | --- | --- |
| results of an unpinned plugin go (any sha counts as pinned) | `gate-host host prune_removes_derived_entries_...` | 101, the test fails |
| blob contents no answer names go | `gate-host host prune_removes_derived_entries_...` | 101, the test fails |
| `--older-than` | `gate-host host prune_removes_derived_entries_...` | 101, the test fails |
| `--older-than` | `gate plugins a_used_result_survives_...` | 101, the test fails |
| least recently used first, by path on a tie (the tie reversed) | `gate-host host max_size_removes_the_least_recently_used_...` | 101, the test fails |
| the pinned-alone-exceed message (`over` never set) | `gate-host host prune_removes_derived_entries_...` | 101, the test fails |
| the pinned-alone-exceed message (`over` never set) | `gate distribution cache_prune_lists_then_removes_only_what_no_lock_pins` | 101, the test fails |
| a result hit sets the last use | `gate plugins a_used_result_survives_...` | 101, the test fails |
| a stored `read-at` answer without content is asked again (the old error back) | `gate plugins a_stored_file_whose_content_...` | 101, the test fails |

The round before it (ticket 241 on `4ef8571`) failed one test, `a_used_result_survives_...`: the test
committed `gate.lock` after its two content commits, so `HEAD~1` read `second`, which is right for
that history; `3d8c4a4` writes the lock first.

After the round, `main` moved to `ec66737` (T-043, T-046, T-049); the branch is rebased onto it, with
two conflicts resolved by hand: `CHANGELOG.md` (both sides' entries kept) and the host test file
(main's `fetch::Bounds` beside this task's `cache::Bounds`, imported as `PruneBounds`). The design's
last sentences, the done-when line on the re-ask and the `CHANGELOG.md` entry are corrected to what
the code and its test show: where git lacks a pruned content too, the call still fails, now with
advice to run `git fetch` (the lead was told when the code showed it).

## Resume point

2026-11-20T01:56:16+00:00: green on the rebased tip `1b141d3` (ticket 277), on `main` `a3dd8d5`. Next: the lead's merge.

2026-11-20T01:32:59+00:00: the review's fix round passed on `27ca81d` (ticket 265, each of the 14 changes caught);
rebased onto `main` `a3dd8d5`. Next: the same round on the rebased tip, then the lead's merge.

2026-11-19T22:51:41+00:00: the full round passed on `3d8c4a4` with each change caught; rebased onto
`main` `ec66737`. Next: the same round on the rebased tip, then the review.

Earlier:

2026-11-19T19:16:49+00:00: the lead approved the design as written; only downloads are pinned,
results of pinned plugins included may go under a bound. Code, tests and documents are pushed; the
first lane round (fmt, clippy, the gate-host and gate tests) is queued. Next: its result, then the
full round with each change taken out.

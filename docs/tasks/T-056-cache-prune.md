# T-056: `gate cache prune` bounds the whole cache

| Field | Value |
| --- | --- |
| Status | in progress (design approved by the lead, 2026-11-19) |
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
does, and the format-2 layout, `results/<2 hex>/`, goes too), and each stored blob content that no
stored `read-at` answer names (`prune` reads `git/answers/` for the ids). A derived entry's last use
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
gone recomputes it (results, `sync-state`) or asks git (blob contents). One change makes the last
true: a stored `read-at` answer whose content neither the cache nor git holds is asked again as if
it were not stored, so a pruned blob content never fails a call (today that call fails and tells the
user to run `gate cache prune`, which does not help).

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
  again and passes; before the change it fails.
- A malformed `--older-than` or `--max-size` is a configuration error, exit 2, that names the form.

The lane: fmt, clippy, the tests above, deny, the release build, and the leak scan, on the pushed
commit; `rustfmt --check` natively before each commit.

## Proof

To be written.

## Resume point

2026-11-19T19:16:49+00:00: the lead approved the design as written; only downloads are pinned,
results of pinned plugins included may go under a bound. Code, tests and documents are pushed; the
first lane round (fmt, clippy, the gate-host and gate tests) is queued. Next: its result, then the
full round with each change taken out.

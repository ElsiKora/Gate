# T-077: `gate cache prune` against a concurrent `gate lock --set`

| Field | Value |
| --- | --- |
| Status | parts one and two reviewed, with one fix round; its lane round waits for T-085 |
| Release | v0.3.0 |
| Writer | Gate 001 |
| Reviewer | a session that did not write it |
| Depends on | `main` at `20480703`; the code lands after T-085, which also touches `crates/gate/src/distribution.rs` |
| Touches | `crates/gate/src/distribution.rs`, `crates/gate-host/src/lock/mod.rs`, `crates/gate/tests/distribution.rs`, `crates/gate-host/src/baseline.rs` and `crates/gate-host/tests/host/findings.rs` (part two), `README.md` (the `gate cache prune` row of the command table, the only text on it), `CHANGELOG.md` |

## Goal

T-076 guards every read-modify-write of `gate.lock` with a file lock in the worktree's git
directory, and it left one question open (its Follow-ups): can a prune that runs at the same time
as `gate lock --set` lose an entry or bring one back? The answer is below. `gate cache prune --yes`
can lose the entry that `--set` has just admitted. `gate baseline prune` neither loses nor brings
back anything of `gate.lock`. The fix makes `gate cache prune --yes` take the same guard.

## Which prune

The question names "a prune", and two commands carry the name. T-076's follow-up names
`gate cache prune --yes`, the only one that removes something `gate.lock` refers to. Both are
covered here: the race is in the first one.

## What each command reads and writes

| Command | Reads | Writes |
| --- | --- | --- |
| `gate lock --set <name>=<url> --file <path>` | `--file` (hashed before the guard), `gate.lock` | `blobs/sha256/<sha256>` in the cache (`admit`), then `gate.lock`. It holds the guard from the read of `gate.lock` until after its write (`distribution.rs:73-82`). |
| `gate lock`, `gate init` | `gate.lock`, the network | downloaded blobs, then `gate.lock`, under the guard |
| `gate cache prune` | the `gate.lock` and `gate.toml` of every worktree of each repository given (`lock::pinned_in`, `distribution.rs:207`), then a listing of the cache (`Cache::plan`, `:213`) | with `--yes`, it removes the planned entries (`Cache::remove`, `:255`). It takes no guard. |
| `gate baseline prune` | `gate.lock` once, when the workspace opens (`workspace.rs:186`), the tracked files, the plugins from the cache | `gate/baseline.json` (`lib.rs:355`) |

`Cache::plan` marks every blob that no checksum read from the locks names as `Reason::Unpinned`,
whatever its age (`cache/mod.rs:494`). Only `tmp/` files get a grace period (a day).

## The interleaving that loses data

1. `gate lock --set house=<url> --file house.wasm` takes the guard, reads `gate.lock`, and admits
   the build: `blobs/sha256/<S>` now exists.
2. `gate cache prune --yes` reads the same `gate.lock`, which does not pin `<S>` yet. It lists the
   cache, finds `<S>` unpinned, and removes it.
3. `gate lock --set` writes `gate.lock` with the pin `house = <url>, sha256 <S>`. It exits 0 and
   says "now in the cache".

Now `gate.lock` pins a blob that is not in the cache, and its release asset is not published yet,
since that is what `--set --file` is for. The next `gate check` fails at exit 2: the download is
refused, and the advice is to publish the asset or run `gate lock --set` again. The run that said
"now in the cache" has been undone. Steps 2 and 3 may come in either order:
the removal needs only that prune reads the lock before step 3 and lists the cache after step 1.

With `gate lock` or `gate init` the same window costs a second download of the blob. Nothing is
lost, since the asset is published.

Nothing is brought back. Prune never writes `gate.lock`, and it never creates a cache entry. A
`gate.lock` read during a write is whole, because T-076 replaces the file with a rename.

### `gate baseline prune`

It reads `gate.lock` once, as a whole file, and never writes it, so a concurrent `--set` leaves it
running with the plugins of the lock it read. That is the same as running it just before the
`--set`, and nothing of `gate.lock` or the cache is lost or brought back. Its own file has a
separate gap, which is not part of this task (see Follow-ups).

## The test that fails today

`a_cache_prune_waits_for_a_lock_set_between_admit_and_write`, in
`crates/gate/tests/distribution.rs`, uses T-076's helpers (`hold_guard`, `spawn_gate`, `waited`):

1. Start from a repository whose `gate.lock` pins the `git` plugin, with the cache tagged.
2. The test acts as `gate lock --set` at step 1: it takes the guard (`hold_guard`) and puts a build
   into the cache with `gate_host::cache::Cache::admit`, which is the real admit path, so
   `blobs/sha256/<S>` exists.
3. Start `gate cache prune --yes` (`spawn_gate`), and read its stderr until the waiting line or its
   end (`waited`).
4. The test writes `gate.lock` with the `house` pin to `<S>`, as `--set` would at step 3, and
   releases the guard.
5. Expected: prune waited and exits 0, and `blobs/sha256/<S>` is still there.

Today prune does not wait. It removes `<S>` and exits, so `waited` sees the end of stderr and
returns false, and the blob is gone. The test fails on "prune waited". Order comes only from the
guard and from prune's own output, never from a sleep, as in T-076.

A second test, `a_cache_prune_without_yes_takes_no_guard`, holds the guard and expects the listing
run (no `--yes`) to finish at once without the waiting line. A listing removes nothing, so it must
not block behind a `gate lock` that is downloading for minutes.

Three more cover the worktrees. `a_cache_prune_waits_for_a_lock_run_in_another_worktree` holds the
guard of a linked worktree only and expects a prune started in the main one to wait for it.
`a_cache_prune_given_one_worktree_by_its_path_and_through_a_symlink_takes_its_guard_once` gives
prune a linked worktree by its path and through a symlink, with no guard held, and expects it to
finish without the waiting line. A guard taken twice by one process waits for itself and never
ends, so the guards are taken once per git directory, keyed by device and inode on Unix (the
canonical path elsewhere). Git resolves the symlink itself, so the unit test
`a_git_directory_reached_through_a_symlink_has_one_guard` checks the key directly.
`a_cache_prune_skips_a_locked_worktree_whose_directory_is_empty` locks a linked worktree and
empties its directory, as when its media is not mounted, and expects prune to skip it and exit 0.
All of them put the run's stderr in their failure messages.

## The fix: prune takes the same guard

1. With `--yes`, `gate cache prune` takes the guard of every worktree of every repository given,
   and holds them all until it has removed its entries: before it reads the first `gate.lock`,
   and through `Cache::plan` and `Cache::remove`. Then a `gate lock`, `gate lock --set` or
   `gate init` in one of those worktrees either finishes first, so prune reads its pin and keeps
   the blob, or starts after, so its admit comes after the removal. The guard is T-076's
   `lock::Guard::take`, unchanged. If a guard is held, prune prints the same waiting line and
   waits.
   **Where the guard lives, and why every worktree.** T-076 put `gate.lock.guard` in the
   worktree's own git directory, `git rev-parse --absolute-git-dir` (`Git::git_dir`). That is
   `<repository>/.git` for the main worktree and `<repository>/.git/worktrees/<name>` for a linked
   one, not the common directory. This matches what it guards: each worktree has its own
   `gate.lock`, and two worktrees writing their own locks never wait for each other. Prune, though,
   reads the `gate.lock` of every worktree of a repository (`git worktree list`), and a
   `gate lock --set` in any of them can admit a blob that only its own lock will pin. Each of those
   runs holds only its own worktree's guard, so prune must hold all of them. One guard in the
   common directory would serialize every worktree's `gate lock` against the others, which is
   wider than T-076 needs, so the layout stays.
2. `lock::Guard::take_worktrees` takes the guards: one per worktree root from
   `git worktree list --porcelain -z`, read as bytes so that a path that is not UTF-8 is still
   guarded and read, of every repository given. Each git directory is taken once, keyed and
   ordered by device and inode on Unix (the canonical path elsewhere), so one directory reached
   through two paths is not locked twice, and two prunes over the same repositories take the
   guards in one order and cannot deadlock. Prune takes them before `lock::pinned_in` reads the
   first lock, and `pinned_in` reads the same listing. No other command takes more than one guard.
   The two listings are two `git worktree list` runs: a worktree added between them is read by
   `pinned_in` but not guarded, so a `gate lock --set` started in it within that window of a few
   milliseconds can still lose its build to the prune.
3. A worktree whose directory is gone (`prunable` in the listing), a bare repository, and a
   worktree whose directory holds no `.git` (a locked worktree on media that is not mounted) have
   no `gate.lock` to pin and no run to wait for, so they are skipped: `Lock::read` already reads it as no lock, and
   `Git::discover` would fail on it. A guard that cannot be
   taken fails the prune, before anything is removed, with T-076's message
   (`cannot lock <guard> ...; check that <git directory> is writable ...`) and "nothing was pruned"
   added.
4. Without `--yes`, prune takes no guard. Its listing may be out of date by the time it prints,
   but it changes nothing.
5. The `gate cache prune` text (the README's command table) adds two points. Prune waits for a `gate lock` that is
   running in a repository it was given. A `--set` pin in a repository it was not given is
   unpinned and is removed, which is unchanged: name every repository that uses the cache.

Holding a guard during the removal blocks a `gate lock --set` in that worktree for the length of
the removal, about the time of a directory walk. I chose this over a prune with no lock (list the
cache first, read the locks second, and admit after `gate.lock` is written), because that one
changes the order inside `Cache::plan` and inside `lock --set`, and T-076 just fixed that order.
The guard is one more caller of a lock that is already tested.

### Mutation check

A step in the lane round removes the guards from prune and expects
`a_cache_prune_waits_for_a_lock_set_between_admit_and_write` and
`a_cache_prune_waits_for_a_lock_run_in_another_worktree` to fail on "prune waited", as T-076's
round checked its guard.

## Part two: a prune that accepts no finding twice

T-077b's `Baseline::pruned_since` (merged in `db96d37f`) keeps an entry written while prune's check
ran, and prunes only the entries the check saw. It pruned those against every finding, so an entry
the check saw could be re-keyed to a finding that an entry written meanwhile already accepts:

1. Prune loads a baseline with a keyless entry `E`.
2. While its check runs, a `gate baseline write` from a tree with both findings keeps `E` and adds
   the keyed entry `E'` for the finding `F`.
3. Prune's check reports only `F`, keyed. Under the guard it re-keys `E` to `F`, and the file holds
   `E'` twice.

Two entries accept two findings of `F`'s fingerprint, so a new one passes the gate: it fails open.
Run after the write, prune would let `E'` accept `F` and remove `E` as stale.

**The fix.** The entries written meanwhile accept their findings first, and the entries the check
saw are pruned against the findings left, so no finding is accepted twice and none is re-keyed to.
The entries written meanwhile are still kept as they are.

**Test.** `a_prune_never_re_keys_an_entry_to_a_finding_an_entry_written_meanwhile_accepts`, in
`crates/gate-host/tests/host/findings.rs`, builds those two baselines and calls `pruned_since`. It
expects nothing re-keyed, `E` removed, the baseline to be `E'` alone, and a check with two copies of
`F` to report one. A second mutation step in the lane round prunes the seen entries against every
finding again and expects the test to fail.

## Done when

- The six tests pass, and the two that wait fail with the guards removed from prune (the
  mutation step).
- Part two's test passes, and fails under its mutation step.
- One lane round on the pushed tip, on this task's own target directory: fmt, clippy, test, deny
  and the release build exit 0, and the log shows `gate-host` and `gate` compiled from this task's
  worktree.
- The leak scan prints nothing.

## Follow-ups

- `gate baseline write` and `gate baseline prune` write `gate/baseline.json` with `fs::write`
  (`lib.rs:277`, `lib.rs:355`), which truncates and then writes. A `gate check` that reads the file
  meanwhile can see it empty or cut short. Two baseline runs at once can lose an update, and with
  prune they can bring back an entry the other run removed: A removes X, B (which read the file
  before A wrote it) removes Y, and B's write puts X back. The fix is T-076's `files::replace`
  plus a guard of its own for the read-modify-write. It touches `gate-host/src/lib.rs` and the
  baseline tests, not `distribution.rs`, so it can be its own task.

## Resume point

2026-11-21T03:56:58+00:00: design approved, on the condition that the note says where the guard lives and why
every worktree is needed; added under "The fix". The baseline follow-up became T-077b, on its own
branch. Next: the code, once T-085 has merged.
2026-11-21T05:51:43+00:00: rebased onto `db96d37f` (T-077b merged). Code and the four tests written:
`Guard::take_worktrees`, prune `--yes` holding its guards, the README row and the changelog entry.
`rustfmt --check` is clean on the changed files; nothing is compiled yet. Next: once T-085 has
merged, rebase onto main and run the lane round with its mutation step.
2026-11-21T05:54:15+00:00: part two, as the lead asked: `pruned_since` lets the entries written meanwhile accept
their findings first, with a host test and a second mutation step in the round script. Next: the
independent review of the branch, then, once T-085 has merged, the rebase and one lane round for
both parts.
2026-11-21T06:01:45+00:00: independent review of `a47c6526`: nothing blocks, with one fix round. A worktree with no
`.git` is skipped, the listing is read with `-z` as bytes, guard directories are keyed by device
and inode, the window between the two listings is stated, and the tests put stderr in their
messages, with a symlink case, an empty locked worktree and a unit test of the key. Next: once
T-085 has merged, the rebase and one lane round for both parts.

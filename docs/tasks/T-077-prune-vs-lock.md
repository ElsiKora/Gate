# T-077: `gate cache prune` against a concurrent `gate lock --set`

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead's approval |
| Release | v0.3.0 |
| Writer | Gate 001 |
| Reviewer | a session that did not write it |
| Depends on | `main` at `20480703`; the code lands after T-085, which also touches `crates/gate/src/distribution.rs` |
| Touches | `crates/gate/src/distribution.rs`, `crates/gate-host/src/lock/mod.rs`, `crates/gate/tests/distribution.rs`, `docs/guide/` (the `gate cache prune` text), `CHANGELOG.md` |

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

## The fix: prune takes the same guard

1. With `--yes`, `gate cache prune` takes the guard of every worktree of every repository given,
   and holds them all until it has removed its entries: before it reads the first `gate.lock`,
   and through `Cache::plan` and `Cache::remove`. Then a `gate lock`, `gate lock --set` or
   `gate init` in one of those worktrees either finishes first, so prune reads its pin and keeps
   the blob, or starts after, so its admit comes after the removal. The guard is T-076's
   `lock::Guard::take`, unchanged. If a guard is held, prune prints the same waiting line and
   waits.
2. `lock::pinned_in` gets a variant that takes the guards: one per worktree root from
   `git worktree list --porcelain`, taken in the order of their paths, so two prunes over the same
   repositories cannot deadlock. No other command takes more than one guard.
3. A worktree whose directory is gone (`prunable` in the listing) has no `gate.lock` to pin and no
   run to wait for, so it is skipped: `Lock::read` already reads it as no lock, and
   `Git::discover` would fail on it. A guard that cannot be
   taken fails the prune, before anything is removed, with T-076's message
   (`cannot lock <guard> ...; check that <git directory> is writable ...`) and "nothing was pruned"
   added.
4. Without `--yes`, prune takes no guard. Its listing may be out of date by the time it prints,
   but it changes nothing.
5. The guide's `gate cache prune` text adds two points. Prune waits for a `gate lock` that is
   running in a repository it was given. A `--set` pin in a repository it was not given is
   unpinned and is removed, which is unchanged: name every repository that uses the cache.

Holding a guard during the removal blocks a `gate lock --set` in that worktree for the length of
the removal, about the time of a directory walk. I chose this over a prune with no lock (list the
cache first, read the locks second, and admit after `gate.lock` is written), because that one
changes the order inside `Cache::plan` and inside `lock --set`, and T-076 just fixed that order.
The guard is one more caller of a lock that is already tested.

### Mutation check

A step in the lane round removes the guards from prune and expects
`a_cache_prune_waits_for_a_lock_set_between_admit_and_write` to fail on "prune waited", as T-076's
round checked its guard.

## Done when

- Both tests pass, and the first fails with the guards removed from prune (the mutation step).
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

2026-11-21T03:54:34+00:00: design note written on `task/T-077-prune-vs-lock` from `20480703`. No
code yet. Next: the lead's approval, then the code once T-085 has merged.

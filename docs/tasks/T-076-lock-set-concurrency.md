# T-076: concurrent `gate lock` runs keep both changes

| Field | Value |
| --- | --- |
| Status | design, for the lead's review |
| Release | v0.3.0 |
| Writer | "Gate 002" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `4c09a042` |
| Touches | `crates/gate-host/src/lock/mod.rs`, `crates/gate/src/distribution.rs`, `crates/gate/tests/distribution.rs`, `CHANGELOG.md` |

## Goal

Two runs that write `gate.lock` at the same time in one worktree, such as two `gate lock --set`
runs, or `gate lock` and `gate lock --set`, must not lose one run's change, and no run may ever
read a half-written `gate.lock`. The second run waits for the first, and says so.

## What is shared, and who reads and writes it

| Shared | Writers | Readers |
| --- | --- | --- |
| `<worktree>/gate.lock` | `gate lock`, `gate lock --set`, `gate init` | `gate check`, `gate sync`, `gate doctor`, `gate cache prune`, the three writers |
| the cache (user-wide) | every command that fetches or admits a plugin, the result and answer caches, `gate cache prune` (removes) | every command that loads a plugin |

What each writer of `gate.lock` does today (`crates/gate/src/distribution.rs`):

- `gate lock --set <name>=<url> --file <path>`: hashes the file, reads `gate.lock`, sets the one
  pin, admits the file to the cache, checks that its hash did not change, writes `gate.lock`.
- `gate lock`: reads `gate.lock`, resolves every release over the network (seconds to minutes),
  writes `gate.lock`.
- `gate init`: the same as `gate lock`, for the core.

Each is a read-modify-write with nothing that orders it against another. `Lock::write` already
writes a temporary file (`.gate.lock.<pid>.tmp` in the worktree root) and renames it over
`gate.lock`, so a reader sees the old file or the new one, never a mix; but it does not sync the
temporary file before the rename, so a crash right after the rename can leave an empty or partial
`gate.lock` on file systems that delay writes.

### The failure today

A lost update. Run A reads `gate.lock`, run B reads the same `gate.lock`, A writes it with its pin,
B writes it with its own: A's pin is gone, and both runs reported success. With `gate lock` the
window is the whole network resolution, so a `gate lock --set` started during it is undone when
`gate lock` writes.

The test below reproduces it without sleeps. Today it fails on "the second run did not wait" and,
after that, on the missing pin.

### The cache is already safe

- Every entry is named by its sha256. A writer creates its own temporary file
  (`tmp/<pid>-<nanos>-<counter>.part`, `create_new`), syncs it, and renames it into place. Two
  runs that store the same plugin write the same bytes under the same name, and the rename is
  atomic, so a reader sees no partial entry.
- A download enters the cache only when its sha256 is the pinned one (`fetch`). A plugin is
  hashed again when it is loaded (`plugins.rs`, "changed after it was verified"). `--file` is
  hashed while it is copied in, and a changed file fails the run with `gate.lock` unchanged.
- `CACHEDIR.TAG` is written with `fs::write` by whichever run comes first, but it is only checked
  for existence and its contents are fixed, so two runs writing it at once do no harm.

So the cache keeps its code. Its fail-closed checks are covered again below, with the new wait.

## The fix

1. **An advisory file lock around read-modify-write of `gate.lock`.** A new
   `lock::Guard::take(&Git)` opens (creating it if needed) `gate.lock.guard` in the worktree's
   git directory (`git rev-parse --absolute-git-dir`, so each worktree has its own, beside its
   `index`, and nothing shows up in `git status`). It locks the file with `File::try_lock`. If
   another run holds it, the run writes to stderr:
   `waiting for another gate run to finish writing gate.lock (<guard path>)`, then blocks in
   `File::lock`. The lock is released when the guard is dropped, and by the operating system when
   a run dies, so a killed run never leaves a stale lock behind. These are std's file locks
   (Rust 1.89 and later; the toolchain is 1.99.1), so there is no new dependency and no `unsafe`.
2. **Who takes it.** `gate lock`, `gate lock --set` and `gate init` take the guard before they read
   `gate.lock` and hold it until after they write it. `--set` hashes `--file` before it takes the
   guard, so a file that changes while the run waits fails the existing "changed while gate read
   it" check, and `gate.lock` stays unchanged. Readers (`check`, `sync`, `doctor`, `prune`) do not
   take it: the rename already gives them a whole file.
3. **When the lock cannot be taken** (the git directory is read-only, the guard path is not a
   file, or the file system does not support locks), the run fails as an environment error and
   says what to do:
   `cannot lock <guard path> to update gate.lock: <error>; check that <git directory> is writable
   and on a local file system, or run one gate lock at a time`.
4. **Atomic replace, completed.** `Lock::write` creates its temporary file with `create_new`,
   syncs it before the rename, and removes it on any error, so a failed write leaves the old
   `gate.lock` in place, and a crash leaves either the old file or the new one.

## Tests

Each is deterministic: order comes from the guard and from reading the second run's stderr, never
from a sleep. A timeout only turns a hang into a failure.

1. `two_lock_set_runs_keep_both_pins` (fails today). The test reads `gate.lock`, takes the guard
   itself as a first run would, and starts `gate lock --set b=<url> --file b.wasm`. It reads the
   child's stderr until the waiting line. It then writes `gate.lock` with pin `a` added to what it
   read, releases the guard, and waits for the child. Expected: the child waited, exits 0, and
   `gate.lock` holds `a` and `b`. Today the child does not wait and writes first, and the test's
   write then removes `b`.
2. `a_waiting_lock_set_whose_file_changed_fails_closed`. The test holds the guard, starts
   `gate lock --set`, reads up to the waiting line, rewrites the `--file` bytes, and releases.
   Expected: the child exits non-zero with "changed while gate read it", and `gate.lock` is byte
   for byte unchanged.
3. `a_guard_that_cannot_be_taken_says_what_to_do`. The guard path is a directory. Expected:
   exit 2 (environment error), the message names the path and the fix, and `gate.lock` is
   unchanged.
4. `a_failed_lock_write_leaves_the_old_file` (unit, `lock/mod.rs`). The worktree root is made
   read-only before `Lock::write`. Expected: an error, the old `gate.lock` byte for byte, and no
   temporary file left.

The existing tests that a checksum mismatch fails closed (`fetch`, the plugin load) keep passing.

## Not in scope, found while reading

- `gate sync` writes generated files in place (`sync/mod.rs`, `fs::write`). A crash during the
  write leaves a truncated file, which `gate sync --check` then reports. The same temporary-file
  write would fix it.
- `gate cache prune --yes` in a repository while a `gate lock --set` in it is between admitting
  the file and writing `gate.lock` can remove the new entry. The pin is then not in the cache
  while its asset is not published yet. Prune could take each worktree's guard while it reads
  that worktree's lock and removes.

## Done when

- The four tests pass, and test 1 fails on the code before the fix.
- One lane round on the pushed tip, on this task's own target directory: fmt, clippy, test, deny
  and the release build exit 0, and its log shows `gate-host` and `gate` compiled from this
  task's worktree.
- The leak scan prints nothing.

## Resume point


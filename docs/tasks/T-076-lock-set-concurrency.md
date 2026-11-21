# T-076: concurrent `gate lock` runs keep both changes

| Field | Value |
| --- | --- |
| Status | in review: review fixes and rebase |
| Release | v0.3.0 |
| Writer | "Gate 002" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `4c09a042` |
| Touches | `crates/gate-host/src/files.rs` (new), `crates/gate-host/src/lib.rs`, `crates/gate-host/src/git.rs`, `crates/gate-host/src/lock/mod.rs`, `crates/gate-host/src/sync/mod.rs`, `crates/gate/src/distribution.rs`, `crates/gate/tests/distribution.rs`, `CHANGELOG.md` |

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

The test below reproduces it without sleeps. Today it fails on the missing pin: the second run
does not wait, writes first, and the first run's write then removes its pin.

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
   and on a local file system`. (There is no unguarded mode to fall back to, so the error offers
   none.)
4. **Atomic replace, completed.** One helper, `files::replace`, writes a temporary file beside
   the target (`.<name>.<pid>.<n>.tmp`, created with `create_new`), gives it the old file's
   permissions, syncs it, renames it over the target, and on Unix syncs the directory, so the
   rename itself survives a crash. On an error before the rename it removes the temporary file,
   so a failed write leaves the old file in place, and a crash leaves either the old file or the
   new one. `files::replace_executable` is the same write with the new file made `0o755` before
   the rename. `Lock::write`, `gate sync`'s generated files, and `gate init`'s `gate.toml` and
   `bin/gate` all go through it (a launcher that runs while `gate init` replaces it reads the old
   file or the new one); there is no second implementation.
5. **Outside a git repository.** `gate lock` and `gate init` cannot run outside a git repository:
   opening the workspace (`Git::discover`) fails before the guard is taken, so the guard always
   has a git directory to live in.

## Tests

Each is deterministic: order comes from the guard and from reading the second run's stderr, never
from a sleep. A timeout only turns a hang into a failure.

1. `two_lock_set_runs_keep_both_pins` (fails today). The test reads `gate.lock`, takes the guard
   itself as a first run would, and starts `gate lock --set house=<url> --file house.wasm`. It reads
   the child's stderr until the waiting line. It then writes what it read with the `git` pin moved
   to another URL, releases the guard, and waits for the child. Expected: `gate.lock` holds the
   moved `git` pin and the new `house` pin, the child waited, and it exits 0. Today the child does
   not wait and writes first, and the test's write then removes the `house` pin.
2. `a_waiting_lock_set_whose_file_changed_fails_closed`. The test holds the guard, starts `gate lock
   --set`, reads up to the waiting line, rewrites the `--file` bytes, and releases. Expected: the
   child exits non-zero with "changed while gate read it", and `gate.lock` is byte for byte
   unchanged.
3. `a_guard_that_cannot_be_taken_says_what_to_do`. The guard path is a directory. Expected: exit 2
   (environment error), the message names the path and the fix, and `gate.lock` is unchanged.
4. The helper's unit tests (`files.rs`):
   `a_replace_writes_the_new_contents_and_keeps_the_old_permissions` (an executable stays
   executable, no temporary file is left),
   `a_replace_executable_makes_a_new_file_executable_before_it_appears`, and
   `a_failed_replace_leaves_the_old_file_and_no_temporary_file`, with the rename failing (the target
   is a directory) and the temporary file failing (the directory is read-only). Expected each time:
   an error that names the path, the old contents, and no temporary file.
5. `a_waiting_lock_resolves_on_top_of_the_first_runs_pins`. As test 1, for plain `gate lock` against
   a test server: the first run moves the `git` pin while `gate lock` waits; `gate lock` then keeps
   it and resolves the missing `house` pin from the server. Without the guard `gate lock` writes
   first and the first run's write removes `house`.
6. `a_waiting_init_resolves_on_top_of_the_first_runs_lock`. The same for `gate init`: the first run
   writes a `gate.lock` that pins the core only; the waiting `gate init` keeps that core pin and
   adds the plugin's.

The existing tests that a checksum mismatch fails closed (`fetch`, the plugin load) keep passing.

## Follow-ups

- T-077, scheduled by the lead after this task merges: `gate cache prune --yes` in a repository
  while a `gate lock --set` in it is between admitting the file and writing `gate.lock` can remove
  the new entry. The pin is then not in the cache while its asset is not published yet. Prune
  could take each worktree's guard while it reads that worktree's lock and removes.

## Known limits

- A run killed with SIGKILL, or a power loss, between creating the temporary file and renaming it
  leaves `.<name>.<pid>.<n>.tmp` beside the target (for `gate.lock`, untracked in the worktree
  root). Nothing removes it, and later writes use other names; delete it by hand.
- On Windows the permissions copied to the new file are the read-only flag only, and a rename
  over a read-only file fails: a read-only `gate.lock` or generated file is not replaced, the run
  fails with the old file left, and clearing the flag fixes it. The directory is not synced there.
  The released binaries are macOS and Linux only.

## Done when

- The tests pass, and the race tests fail on the code without the guard: a mutation step in the
  lane round removes the guard from `gate lock --set`, `gate lock` and `gate init` and expects
  tests 1, 5 and 6 to fail.
- One lane round on the pushed tip, on this task's own target directory: fmt, clippy, test, deny
  and the release build exit 0, and its log shows `gate-host` and `gate` compiled from this
  task's worktree.
- The leak scan prints nothing.

## Resume point

2026-11-20T19:20:55+00:00: design approved with two additions (the sync writes through the same
helper; the outside-git case stated). Built: `files::replace`, `lock::Guard`, `Git::git_dir`, the
guard taken by `gate lock`, `gate lock --set` and `gate init`; tests 1 to 3 in
`crates/gate/tests/distribution.rs`, test 4 in `files.rs`; the CHANGELOG entries. Next: one lane
round on the pushed tip, then independent review.
2026-11-20T21:47:35+00:00: lane round 381 on `390a0d82` (round 376 on `09405e32` stopped at
clippy: two redundant `std::process::Stdio` paths, fixed in `390a0d82`): fmt, clippy, test, deny,
the release build and the 13 plugin wasm builds exit 0; `gate-host` and `gate` compiled from this
task's worktree; the five new tests ran and passed. The mutation step (the guard removed from
`gate lock --set`) made test 1 fail on "the second run's pin is not lost", exit 101. Next:
independent review.
2026-11-21T00:49:08+00:00: review accepted pending a round after the rebase. Rebased onto `main`
at `2ef2cc61` without a conflict; the review fixes in `0a3acfdd` (race tests 5 and 6, the guard
error, the directory sync, `bin/gate` and `gate.toml` through the helper, a separator-agnostic
path check, the known limits). Lane round 394 on `0a3acfdd`: fmt, clippy, deny, the release build
and the 13 plugin wasm builds exit 0; test exit 101 on one test only,
`a_monorepo_root_extends_git_recommended_and_its_own_rules_win` (`crates/gate/tests/plugins.rs`),
which fails the same way in other rounds on `main`; every test of this task passed. The mutation
step (all three guards removed) made tests 1, 5 and 6 fail, exit 101 each: test 1 on the lost
pin, tests 5 and 6 on "waited". Next: the merge.

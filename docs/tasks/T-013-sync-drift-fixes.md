# T-013: Sync-drift and sync fixes from the T-003 fix-round review

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 002: plugin form spike" |
| Reviewer | a separate review session |
| Depends on | T-003 (merged) |
| Touches | `wit/gate.wit`, `crates/gate-sdk/**`, `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/workspace.rs`, `crates/gate-host/src/sync/**`, `crates/gate-runtime/src/runtime.rs`, `crates/gate/src/main.rs`, their tests |

## Goal

`git.sync-drift` answers the same at a commit whatever the run's network state, and only changes when an
input it depends on changes. `gate sync` never deletes a file and then refuses. History rules over a long
history cost only the new commits after the first run.

## Scope

1. **Cache key on the commit's own path.** When a generator fails at a commit, the answer does not use
   the run's outputs. Use the paths the commit's own configuration names for that generator, or record
   that the answer used the run's configuration and key it with the run configuration's hash.
2. **Per-project failure.** A generator that fails in one project adds only that project's outputs.
   Group failures and run outputs by project and generator, not by generator alone.
3. **When the fallback applies.** Fall back to the run's configuration only when the commit's
   `gate.toml` is not a configuration of this core: it does not parse, or it does not validate. Any other
   error on the commit's own path keeps the commit's own path:
   - A plugin pinned at the commit that cannot be fetched or verified fails the call. The error names the
     plugin, the version and what to do.
   - A `sync::plan` error is a generator error. It answers the paths it could not produce, on both paths,
     and never fails the call.
4. **Cost per commit.** Compute the run's workspace, configuration hash, lock check, loaded plugins and
   validation once per run, as `run_outputs` already is, not once per commit. Report the time for a
   history check over a synthetic history of at least 1,000 commits, on the first run and on the second.
5. **No partial sync.** `stale()` skips protected paths, and `gate sync` checks the whole write and
   removal list against the guard before it writes or removes anything. Test with a repository generator
   that owns `docs/**` and a nested project that has `docs/site/gate.toml`.
6. **`gate run --target-ref` without `--push`** is refused, with the same message as `check`.
7. **`--target-ref refs/`** is refused: a ref needs a name after `refs/`.
8. **The fallback key covers `gate.lock`**, so re-pinning a plugin to a different checksum does not reuse
   fallback answers.

9. **`gate sync` takes `--plugin <name>=<path.wasm>`**, with the same rules as `check`: for that run
   only, an unpinned plugin is allowed, and the lock check skips the plugins an override covers. Both
   `sync` and `sync --check` take it. Without it, a changed generator cannot be proved locally, and an
   acceptance run cannot run `sync --check` without fetching. The written files are the generator's
   output, as for a pinned plugin. architecture.md §16 now names both commands.
10. **An `off` rule needs no options.** `check_rule` in `crates/gate-host/src/workspace.rs` validates
    absent options as `{}` against the schema even when the rule is off in every aspect. So a rule with
    a required option cannot be turned off, even when a preset enables it. When the rule is off and
    carries no options, skip the schema check. Options it does carry are still validated. Test both
    cases, and a preset that enables the rule while the project turns it off.
11. **`norms` carries every norm.** `wit/gate.wit` says `check-request.norms` is "every norm the
    configuration maps". `fn norms` in `crates/gate-host/src/plugins.rs` passes only the norms of the
    rules in the current call. So `agents/rule-enforcement` sees only `agents/*` norms, and it blocks a
    correct tree whose rule document cites a norm set on a `git/*` rule. Pass the norms of every rule
    configured for the scope (every plugin and `gate/*`), whatever the rule's level. Test through the
    real host with two plugins.
12. **`sync-drift` becomes `sync-state`.** In `wit/gate.wit`, `sync-state: func(commit: string) -> sync-state`,
    where `record sync-state { produced: list<string>, drift: list<string> }`. `produced` is every path
    `gate sync` writes or owns at the commit: generator outputs, tracked files under an owned glob, and
    the core's hook and CI files. `drift` is the produced paths that differ, as `sync-drift` answers
    today. Both lists use the same fallback, cache key, grant filter and read-set record. With this,
    git/main-history can tell an in-sync generated file from a hand-written one without its own
    `generated` list, which the T-008 review found could disagree with the generators. Rename it in the
    SDK and the runtime, and tell Gate 004 the commit: T-008's main-history moves to the new call.

Not in scope: surfacing generator findings during `gate sync` as `gate/sync#<generator>`, and persisting
other commit answers. Both stay on the plan's list for v0.1.0.

## Done when

- A test for each item above, failing before the fix and passing after it.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0, run on the pushed commit.
- The timings for item 4.

## Notes

- Source: the independent review of the T-003 fix round found no blocker for the
  merge. These items are kept for this task so the merge did not wait. They must land before rc.1 because
  the first consumer dry run uses the history rules on a long history.
- The rule is architecture.md, the bullet "`sync-state` at a commit".

## Resume point

2026-11-17: items 1 to 12 are written with their tests. Item 12: the WIT record is `sync-paths`,
not `sync-state`, because an interface's types and functions share one namespace and wit-parser
refuses the duplicate. The function is `sync-state` and the SDK type is `SyncState { produced, drift }`.
`produced` is every output plus every path sync would remove; `drift` is a subset of it.

Item 4 timings, from `timing.sh` (outside the repository) with a release build and a fresh
cache. The history has 1,000 commits: 200 whose `gate.toml` is not this core's (the run's configuration
answers), then 800 with Gate's configuration and a hand edit every 100th commit. `probe/git` asks sync-state for each commit and
reports "1000 commits, 208 drift" on both runs. The machine's load was 53 to 75 (`uptime`).
- First run: 119.26 s, 1,200 answers stored (200 commits x 2 entries, 800 x 1).
- Second run: 1.36 s, every answer from the cache.

Review fix round (the review):
- A commit whose gate.lock does not match its gate.toml falls back to the run's configuration.
- An off rule is the level the run resolves (the rule's, else its family's) with no aspect on.
- A run-dependent answer is keyed by the run configuration's hash and the paths the run's
  generators write on the run's tree.
- A generator's passing failure (it answers `err`, traps, or the runtime cannot start) answers
  for this run only and is not stored. The runtime reports an `err` answer as its own kind,
  `Reported`; a deterministic failure (load, interface break, fuel, memory) is a configuration
  error and is stored.
- Only a fetch or checksum failure at a commit fails the call. A component that fetched but does
  not load under this core is left out: its generators answer their paths, the CI workflow
  answers as drifting, and the commit's rules are not validated against it.

Round: every gate exit 0, 251 tests; timings on its release build: 86.77 s first run,
1.24 s second (same history and script).

Merged to main by the merge, with every gate exit 0 and 295 tests (the merge
message records them). The verification review found no blocker; its remaining findings are on
the plan's "Open after rc.1" list.

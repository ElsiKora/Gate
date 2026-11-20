# T-047: a run compiles only the plugins it needs

| Field | Value |
| --- | --- |
| Status | redesigned by the lead after review: validation is never narrowed, only execution |
| Release | 0.2.1, from the `v0.2.0` tag |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | none (host code; T-037 is plugin code and merges apart) |
| Touches | `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/lib.rs`, `crates/gate-host/src/workspace.rs`, `crates/gate-host/src/cache/mod.rs` (the stored declarations), `crates/gate-runtime/src/runtime.rs` (item 7); their tests in `crates/gate/tests/plugins.rs`; sections 4 and 9 of `docs/architecture.md` |

## Goal

Today every command that loads plugins fetches, compiles and describes every plugin the
configuration declares (`plugins::load` and `instantiate` in `crates/gate-host/src/plugins.rs`),
whatever `--rule` and the change selection leave to run. On an export of a consumer repository's
NestJS service, in lane ticket 205 (Gate 005), `bin/gate check --rule 'nestjs/*' --timings` with the released 0.2.0 core and
plugins, network denied, load average about 51, measured these figures:

| Run | Wall | Gate total | Load (wasm compile) | nestjs calls |
| --- | --- | --- | --- | --- |
| cold | 7.12 s | 6.070 s | 4 852 ms | 981 ms |
| warm | 0.49 s | 0.334 s | 87 ms | cached |

On that cold run, `load` was 80 % of the gate's own time: it compiled all seven configured plugins
to run the rules of one. The goal: a plugin that has no rule this run selects and no generator this
command needs is never fetched, compiled or described.

## Redesign after review (the lead, 2026-11-19)

The review of `a6870b6` showed that the first design narrowed validation along with execution. A
plugin that a run did not load lost every check that reads what it declares, not only its option
schema: its `[generate]` names (the T-017 error, which must fire for every command), its `[rules]`
ids, aspects and scope placement, rule options and exceptions, and `check_needs`. Two checks that
need no plugin were skipped too: one plugin declared with two sources (in the review's list, also
an exception whose plugin is not declared where its file applies; that one never was: the
configuration load refuses it in `load_exceptions`, for every command). Every later check of that kind (T-048's decision
check) would also pass silently in `--rule` and `--staged` runs.

The lead changed the design: **validation is never narrowed; only execution is.**

- **Checks that need no plugin** run for every declared plugin, before any plugin is skipped.
- **A stored declaration per plugin.** What a plugin declares when it describes itself is written
  to the cache whenever it loads: its rules (id, scope, aspects, options schema, history), its
  presets, its option schema and its generators. `NEEDS` travels inside each rule's options
  schema. The entry is keyed by the plugin's sha256, the plugin interface and the core version,
  so a core that reads a declaration differently never reads another core's entry. Every run
  validates the whole configuration: against the loaded plugins, and against the stored
  declarations of the plugins it does not run. When a skipped plugin's declaration is missing or
  unreadable, the run loads that plugin to get it. `gate sync` loads every plugin, so after a sync
  no check compiles a plugin it does not run.
- **No skip of checks.** `Loaded::skipped` is gone. The checks read declarations, which are always
  complete. The one remaining skip is a plugin that a commit's configuration pins and that does
  not load under this core for good (`commit_plugins`), whose generators cannot produce. Even
  there, the checks that need no plugin still run.
- **The trade test flips.** An option that plugin `b`'s schema refuses fails `--rule 'a/*'` too.
- **Known cost.** A `sync-state` answer at a commit whose own configuration validates still loads
  every plugin that commit pins (`Needed::Every` in `commit_plugins`), as before T-047.

## Scope

1. **What a check runs.** Before anything loads, `gate check` (and so `--hook`, the baseline
   commands and `gate run`'s check steps) computes the set of plugins it runs, from what it already
   knows without a plugin:
   - the scopes the selection runs (the repository scope when `selection.repository` is set, and
     each selected project);
   - the plugins each of those scopes declares (`settings.plugins`);
   - the plugins `--rule` allows (`RuleFilter::allows_plugin`).

   Only those plugins are fetched, verified against their pins, compiled and described, in
   parallel as today. Every other declared plugin gives its stored declaration instead, or is
   loaded when it has none. `--list-changed-projects` still loads none.
2. **Generators on demand.** A rule that asks `git.sync-state` can fall back to the run's
   configuration (`GitHistory::compute`), and that fallback needs the generators of the run's
   plugins. `Loaded` loads a generator's plugin the first time a fallback needs it, once per run.
   A run with no such fallback loads none of them.
3. **Validation is whole.** Every run checks the whole configuration, as `gate sync` does: the
   checks that need no plugin for every declared plugin, and every check that reads a declaration
   against the loaded plugins and the stored declarations together. A `--rule` key is checked
   against every declaration, so `needed` has no branch for the plugin a key names.
4. **One way.** There is no option to load every plugin again. `plugins::load` takes the set it
   runs; there is no second loader, and validation has one path.
5. **Sections 4 and 9 of the architecture** say what a run loads, that every run validates the
   whole configuration, which command fills the compiled-module cache, and the known cost above.
6. **The `commit-msg` hook stays as it is here.** The hook the guide shows is
   `commit-msg = ["check --commit-message"]`. With no `--staged` and no `--rule`, that check is
   `ChangeRequest::All` on the work tree (`check` in `crates/gate/src/main.rs`): it checks the
   tree and the message, so under item 1 it still loads every plugin a scope declares. The lead
   decided on 2026-11-19 that what it checks changes in its own task: T-057, below. This task's
   Proof measures the `commit-msg` row as the consumer's entry stands.
7. **wasmtime's compile settings.** Today the engine (`Runtime::new` in
   `crates/gate-runtime/src/runtime.rs`) sets neither, so it runs on wasmtime 50.0.2's defaults:
   parallel compilation on (the workspace enables the `parallel-compilation` feature, and
   `Config::parallel_compilation` defaults to true), and the cranelift opt level `Speed`. Measure,
   on the cold consumer run `bin/gate check --rule 'nestjs/*' --timings`, these variants:
   - today's settings;
   - parallel compilation off;
   - opt level `None`;
   - opt level `SpeedAndSize`.

   Record each variant's `load` time and the `nestjs` call's time (fuel does not change with
   either setting; the wall time of the calls does). Each variant is a measurement build that is
   not committed. If a setting lowers the cold total without raising the warm total, it goes into
   `Runtime::new` in this task, with the numbers in the commit. A module compiled under one setting
   is a different cache entry, so the change is safe for caches that already hold modules.

Out of scope: a finer cut that skips a declared plugin whose rules are all `off` in the running
scopes. To know which of its rules are on needs its presets, and so its `describe`, which is the
load itself.

## Recommendation: the command that fills the compiled-module cache

The question: should `gate lock`, or the first `gate sync`, also fill the compiled-module cache,
so a first commit hook never pays the compile?

**Approved by the lead on 2026-11-19: `gate sync`, every time it runs, loads every plugin `gate.lock` pins. It is
the one command exempt from item 1. No flag, no "first" state.**

- **Why `gate sync`.** It is the command every clone runs before its first hook can fire, because
  it sets `core.hooksPath` and writes the hooks. It runs on the machine whose hooks will run, and
  the compiled-module cache is per machine: it lives in the user cache directory, keyed by plugin
  sha256, wasmtime version and target. So after `gate sync`, every plugin is compiled for that
  machine, and the first hook only loads cached modules. Section 3 of the architecture puts a
  cached module's load at 0.9 ms. Today sync already loads every plugin (`PluginGenerators::load`),
  so this keeps what sync does and writes it down as its job, instead of letting T-047 shrink it to
  the generator plugins.
- **Why not `gate lock`.** It runs on one machine, the one that moves a pin, and only then. Other
  clones and CI runners never get its cache. `gate lock --set` downloads nothing to compile.
- **Why every sync, not only the first.** On a warm cache, each plugin is a cache hit, so a sync
  pays almost nothing for it. Running only on the first sync would need stored state to know a
  sync was the first, and that state would be one more thing that can go stale.
- **Why load and not only compile.** Describing a compiled plugin costs one instantiation and one
  call. In return, sync checks the whole configuration against every plugin's schemas. Without
  it, item 3 would leave that check to CI. Loading also keeps a single path, `instantiate`; a
  compile-only path would be a second loader.
- **What stays open.** A core upgrade that changes the wasmtime version changes every module's
  key. The first hook after that upgrade arrives by a pull compiles again, unless `gate sync` ran
  first. With item 1, that hook compiles only the plugins it needs. The release notes of a core
  that moves wasmtime say to run `gate sync`. No flag is added for it.

## Tests

Each test fails on `a6870b6`, the first design, unless it says otherwise. They go in
`crates/gate/tests/plugins.rs`, with synthetic plugins and trees:

- A `[generate]` key that names no generator of its plugin fails `check --rule 'a/*'` and a
  `check --staged` that does not select the project declaring that plugin.
- A `[rules]` id that names no rule of plugin `b` fails `check --rule 'a/*'`.
- Plugin `b` declared with two sources fails `check --rule 'a/*'`.
- An exception that names a rule plugin `b` does not declare fails `check --rule 'a/*'`.
- A `--rule b/<typo>` key fails a `check --staged` that does not run `b`, and a key that names a
  rule `b` declares passes it. This one passes on `a6870b6` too, whose `needed` loaded the plugin
  a key names; it guards the removal of that branch.
- A declared plugin `b` whose component does not load, and so has no declaration, fails
  `check --rule 'a/*'`: the run loads it to get the declaration.
- The trade test, flipped: an option `b`'s schema refuses fails `gate sync`, a full `gate check`
  and `gate check --rule 'a/*'`.
- After `gate sync` and an emptied module cache, `check --rule 'a/*'` compiles only `a`, and a
  `check --staged` compiles only the plugins of the scopes it selects. These guard against a
  change that loads every plugin to validate; they pass on `a6870b6`.
- Kept from the first design: the `sync-state` fallback loads the generator plugin a `--rule`
  filter left out, and after `gate sync`, no check compiles a plugin again.
- Unit tests: a stored declaration reads back as written and only under its plugin's sha256, an
  entry that does not parse reads as missing, and `gate cache prune` keeps the declarations of
  pinned plugins only.

From the review of `f7269b4` (the lead, 2026-11-20). Each fails with its piece removed; the round
removes each piece in turn and runs the test, and the Resume point records the exits:

- `an_unknown_preset_fails_a_run_that_does_not_run_its_plugin`: an `extends` entry naming no
  preset of `b` fails `gate check` and `gate check --rule 'a/*'` (piece: `check_presets` in
  `validate`).
- `a_rule_need_follows_the_rule_filter`: a missing need of `b`'s rule fails `gate check`, and
  `gate check --rule 'a/*'` passes (piece: the filter in `check_needs`).
- `a_sync_state_fallback_checks_rule_needs_with_the_run_s_rule_filter`: the same need fails a full
  check, and a `--rule` run whose `sync-state` answer falls back to the run's configuration gives
  the same answers as without `b` (piece: the run's filter in `run_context`).
- `a_stored_declaration_that_is_corrupt_or_names_another_plugin_is_replaced_by_the_run_that_loads_it`:
  for an entry that does not parse and one that names another plugin, the next run loads `b`,
  writes the entry again, and the run after compiles nothing for `b` (pieces: the name filter in
  `load`, and `declarations::write` placing over an existing entry).
- `a_stored_declaration_whose_rule_schema_is_not_json_loads_the_plugin`: such an entry is not an
  error; `b` loads (piece: a `Declaration::of` failure read as a miss).
- `after_gate_sync_a_pinned_plugin_the_run_does_not_run_answers_from_its_stored_declaration`: a
  pinned download left out by `--rule` compiles nothing (piece: the read of stored declarations).
- Unit tests in `plugins/declarations.rs`: the entry name is `key_for(format, interface,
  version)` and changes with each of the three; a stored entry is replaced when written again.

## Done when

- The before/after measure, lane ticket 267 (Gate 005), 2026-11-20T00:41:02 to 00:41:47 UTC,
  load average 54 to 71 on 18 CPUs: the released 0.2.0 core against the release build of lane
  ticket 256 (`65e5a55`, sha256 `3eb84b41…`), on an export of a consumer repository's NestJS
  service, network denied, one copy per binary and one run per row. The check rows ran
  `gate check --rule 'nestjs/*' --timings`; the hook rows ran the `commit-msg` hook, which prints
  no timings, so only wall time is given. Exit 1 on a check row is the tree's findings, the same
  for both. "Modules emptied" means `modules/` was removed from the cache after `gate sync`.

  | Row | 0.2.0: exit, wall, total (`load`) | this branch: exit, wall, total (`load`) | modules compiled |
  | --- | --- | --- | --- |
  | after sync, modules emptied, check | 1, 7.00 s, 6 944 ms (5 126 ms) | 1, 2.74 s, 2 664 ms (1 475 ms) | 8 → 2 |
  | after sync, warm check | 1, 1.50 s, 1 438 ms (765 ms) | 1, 0.77 s, 674 ms (107 ms) | 8 → 2 |
  | `gate sync` | 0, 6.52 s | 0, 4.82 s | 8 → 8; declarations stored 0 → 7 |
  | after sync, modules emptied, `commit-msg` hook | 0, 1.31 s | 0, 0.84 s | 0 → 0 |
  | first run, empty cache, check | 1, 4.20 s, 4 126 ms (3 006 ms) | 1, 5.10 s, 4 997 ms (3 797 ms) | 8 → 8 |
  | first run, empty cache, `commit-msg` hook | 0, 1.11 s | 0, 1.39 s | 0 → 0 |

  After `gate sync` a check compiles the plugins it runs, 2 of 8 here. A first run on an empty
  cache loads every plugin either way and gains nothing; one sample at this load is noise below
  about 0.5 s.
- The table of item 7: what is on today, each variant's `load` and `nestjs` call times cold, and
  what this task changed, with the command and the lane ticket.
- The tests above, each failing before its change where the list says so.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Proof

### Item 7: wasmtime's compile settings

Lane ticket 243 (Gate 005), 2026-11-19T20:59:06 to 20:59:40 UTC, exit 0. The script was
`measure-consumer.sh`: `GATE_T047_COMPILE=<variant> GATE_CACHE_DIR=<cache> gate check --rule
'nestjs/*' --timings` on the consumer copy, network denied. Each variant ran once cold (a fresh
cache seeded with the released archives only) and once warm. The binary was the measurement
build of lane ticket 233: `93749ae` plus the uncommitted `GATE_T047_COMPILE` switch in
`Runtime::new`, sha256 `818f5066…`. That build compiles `nestjs` alone, so a cold row is one
plugin's compile, not lane ticket 205's seven. The machine's load average was 46 to 51 on 18
CPUs, with one sample per row. Exit 1 in every row is the tree's findings. Fuel was equal in
every variant.

| Variant | Run | Wall | Gate total | `load` | `plugins` |
| --- | --- | --- | --- | --- | --- |
| today (parallel on, opt `Speed`) | cold | 3.81 s | 3 749 ms | 1 342 ms | 2 203 ms |
| today | warm | 0.29 s | 229 ms | 60 ms | 6 ms |
| parallel compilation off | cold | 16.53 s | 16 288 ms | 12 920 ms | 2 707 ms |
| parallel compilation off | warm | 0.36 s | 299 ms | 62 ms | 6 ms |
| opt level `None` | cold | 5.67 s | 5 519 ms | 2 250 ms | 3 123 ms |
| opt level `None` | warm | 0.40 s | 321 ms | 96 ms | 6 ms |
| opt level `SpeedAndSize` | cold | 4.58 s | 4 519 ms | 2 562 ms | 1 742 ms |
| opt level `SpeedAndSize` | warm | 0.33 s | 253 ms | 67 ms | 11 ms |

What is on today stays: no variant lowered the cold total. Parallel compilation off is about ten
times slower to load (12 920 ms against 1 342 ms). Neither opt level beat `Speed` cold. The gaps
between `Speed`, `None` and `SpeedAndSize` are within the noise of one sample at a load near 50.
`Runtime::new` is unchanged, and the switch was never committed.

## Open items

- **T-057, v0.3.0: a `--commit-message` check checks only the message.** The lead decided this
  on 2026-11-19. A check with `--commit-message` runs only the rules that declare they read the
  message being written, through `Rule::NEEDS` with the message. It does not use `reads-history`
  for this. The tree stays with `pre-commit`, `pre-push` and CI. With item 1, that check then
  loads only the plugins of those rules. It changes what an existing `commit-msg` hook checks, so
  it ships in v0.3.0 and is listed under Changed in `CHANGELOG.md`. The getting-started and
  configuration pages of the guide change with it. The writer of this task writes T-057's task
  file after T-047.

## Resume point

2026-11-20T01:44:56+00:00:
- Design: the lead's redesign above (validation never narrowed; stored declarations keyed by
  entry format, interface and core version). The first design (`a6870b6`) was reviewed fix first
  and replaced.
- Overlap with T-056 (`gate cache prune`, Gate 004), decided by the lead: whichever branch merges
  second puts `declarations/` into T-056's derived class, like `results/<plugin sha256>/`; an
  entry read is touched; the interim rule in `Cache::unreferenced` and its unit test go. Agreed
  with Gate 004 against T-056's `96552d0`: a declarations loop in `Cache::plan` mirroring the
  results loop, before the `older_than` block, with a declarations case in
  `prune_removes_derived_entries_nothing_can_use_and_bounds_never_remove_a_pinned_download`.
- The review of `f7269b4` (2026-11-20) said fix first. Its points are in: presets checked for
  every declared plugin in every scope in `validate`; needs follow `--rule` on both paths; a
  stored entry is always placed; `key_for`; architecture section 7's validation list and
  section 9's trust sentence; the tests above; the `CHANGELOG.md` entry under Performance; the
  measure table in Done when. Point 6 (`with_generators` loading only the missing generator
  plugins) is not a small change: a `Loaded` holds one runtime, and a plugin is instantiated by
  the runtime that compiled it, so the run's plugins load again with them (from the module
  cache); a comment there says so. The branch is rebased onto main `a3dd8d5`.
- Rounds:
  - First design: ticket 224 (tests on `ba462ff`; two expected failures, a fixture fixed in
    `9bb5b36`), ticket 226 (clippy `similar_names`, fixed in `a6870b6`), ticket 240 withdrawn.
  - Item 7: ticket 233 (measurement build), ticket 243 (variants; wasmtime's defaults stay).
  - Redesign: ticket 242 (the new tests fail on `7e904e4`, exit 101; clippy E0027 and
    `unused_qualifications`, fixed in `bb7fc5d`), ticket 251 (1025 passed, 1 failed: the exception
    test, rewritten in `65e5a55`), ticket 256 (the rewritten test fails before the change, exit
    101; on `65e5a55` fmt, clippy, test 1026 passed 0 failed, deny and the release build exit 0;
    binary sha256 `3eb84b41b421a3980363a9a4dd05b0a2321bea6a9b99c55479fe20db32922425`).
  - Measure: ticket 267 (before/after, in Done when).
- Next: the review round, which removes each reviewed piece in turn and runs its test, then the
  gates on the pushed tip.

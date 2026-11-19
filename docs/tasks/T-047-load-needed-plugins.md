# T-047: a run compiles only the plugins it needs

| Field | Value |
| --- | --- |
| Status | design approved by the lead; tests and the change pushed, gates queued |
| Release | 0.2.1, from the `v0.2.0` tag |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | none (host code; T-037 is plugin code and merges apart) |
| Touches | `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/lib.rs`, `crates/gate-runtime/src/runtime.rs` (item 7); their tests in `crates/gate/tests/plugins.rs`; section 9 of `docs/architecture.md` |

## Goal

Today every command that loads plugins fetches, compiles and describes every plugin the
configuration declares (`plugins::load` and `instantiate` in `crates/gate-host/src/plugins.rs`),
whatever `--rule` and the change selection leave to run. On the consumer tree, in lane ticket 205
(Gate 005), `bin/gate check --rule 'nestjs/*' --timings` with the released 0.2.0 core and
plugins, network denied, load average about 51, measured these figures:

| Run | Wall | Gate total | Load (wasm compile) | nestjs calls |
| --- | --- | --- | --- | --- |
| cold | 7.12 s | 6.070 s | 4 852 ms | 981 ms |
| warm | 0.49 s | 0.334 s | 87 ms | cached |

On that cold run, `load` was 80 % of the gate's own time: it compiled all seven configured plugins
to run the rules of one. The goal: a plugin that has no rule this run selects and no generator this
command needs is never fetched, compiled or described.

## Scope

1. **What a check needs.** Before anything loads, `gate check` (and so `--hook`, the baseline
   commands and `gate run`'s check steps) computes the set of plugins from what it already knows
   without a plugin:
   - the scopes the selection runs (the repository scope when `selection.repository` is set, and
     each selected project);
   - the plugins each of those scopes declares (`settings.plugins`);
   - the plugins `--rule` allows (`RuleFilter::allows_plugin`).

   Only those plugins are fetched, verified against their pins, compiled and described, in
   parallel as today. `--list-changed-projects` still loads none.
2. **Generators on demand.** A rule that asks `git.sync-state` can fall back to the run's
   configuration (`GitHistory::compute`), and that fallback needs the generators of the run's
   plugins. `Loaded` keeps one slot per declared plugin and loads a generator's plugin the first
   time a fallback needs it, once per run. A run with no such fallback loads none of them.
3. **Validation follows the load.** The configuration of a plugin that did not load is checked by
   the core alone: its section, its source and its pin, its rule ids against `[rules]`. It is not
   checked against the plugin's own option schemas. That is `validate_except` with the plugins
   not loaded as `skipped`, as the commit-plugin path already does. A `--rule` key that names a
   plugin not declared anywhere still fails, as now. The command that checks the whole
   configuration against every plugin is `gate sync` (see the recommendation), and so is a full
   `gate check` in CI, which needs every plugin.
4. **One way.** There is no option to load every plugin again. `plugins::load` takes the set it
   should load; there is no second loader.
5. **Section 9 of the architecture** says what a run loads, and which command fills the
   compiled-module cache.
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

Each test fails before its change and passes after it. They go in `crates/gate/tests/plugins.rs`,
with synthetic plugins and trees:

- With `--rule 'a/*'` and two declared plugins, where `b` is overridden with bytes that are not a
  component, the check runs `a` and exits as `a`'s findings say. Before the change, it fails
  loading `b`.
- In `check --staged`, a change only in project `p1`, which declares only `a`, does not load `b`,
  which is declared only in `p2` and is broken as above. A change in `p2` loads `b` and fails as
  now.
- A `git.sync-state` fallback to the run's configuration still answers as before, with the
  generator's plugin loaded on demand.
- After `gate sync` on an empty `GATE_CACHE_DIR`, a `gate check` adds no file to the module cache.
  This test fails if sync stops loading every pinned plugin.
- The trade of item 3, pinned: a plugin `b` with an option its schema refuses fails `gate sync`
  and a full `gate check`, and does not fail `gate check --rule 'a/*'`.

## Done when

- The consumer measure, by Gate 005 or the lead, run before (the released core) and after (this
  branch's build), cold and warm: `bin/gate check --rule 'nestjs/*' --timings`. The table records
  each run's exit, wall time, gate total and `load` time, with the command and the lane ticket.
  The expected change is in `load` cold: one plugin's compile instead of seven at once. How far it
  falls is what the measure says.
- The hooks in the same table: the consumer's `commit-msg` entry as it stands, run cold with
  `--timings` on a message file, before and after (item 6).
- The table of item 7: what is on today, each variant's `load` and `nestjs` call times cold, and
  what this task changed, with the command and the lane ticket.
- The tests above, each failing before its change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

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

2026-11-19T19:09:22+00:00:
- The lead approved the design. The branch was cut from main `67bbd66`.
- `ba462ff` holds the three tests of items 1 and 3, written before the change.
- `a491f40` holds the change, items 1 to 4. A `sync-state` answer at a commit whose own
  configuration validates still loads every plugin of that commit (`Needed::Every` in
  `commit_plugins`), as before.
- Item 6 went to T-057.
- Lane ticket 224 runs the three tests on `ba462ff` in their own worktree; each is expected to
  fail. Ticket 226 runs the gates on the change.
- Next: the `sync-state` fallback test and the `gate sync` cache test, item 5, item 7's
  measures, then the consumer measure.

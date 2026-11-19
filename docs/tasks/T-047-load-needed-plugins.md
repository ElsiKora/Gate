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

## Redesign after review (the lead, 2026-11-19)

The review of `a6870b6` showed that the first design narrowed validation along with execution. A
plugin that a run did not load lost every check that reads what it declares, not only its option
schema: its `[generate]` names (the T-017 error, which must fire for every command), its `[rules]`
ids, aspects and scope placement, rule options and exceptions, and `check_needs`. Two checks that
need no plugin were skipped too: one plugin declared with two sources, and an exception whose
plugin is not declared where its file applies. Every later check of that kind (T-048's decision
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
- An exception whose plugin is not declared where its file applies fails `check --rule 'a/*'`.
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
- The tests above, each failing before its change where the list says so.
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

2026-11-19T20:21:46+00:00:
- First design: `ba462ff` (tests), `a491f40` (the change), `93749ae` (guards and section 9),
  `9bb5b36` (the trade test's fixture), `a6870b6` (clippy).
- Lane ticket 224 ran the tests on `ba462ff`: the two load tests failed as expected (exit 101); the
  trade test failed on its own fixture, fixed in `9bb5b36`. Ticket 226 stopped at clippy
  (`similar_names`), fixed in `a6870b6`. Ticket 240 was withdrawn before it started, when the
  lead changed the design.
- The review of `a6870b6` said fix first; the lead's redesign is above.
- Ticket 233, item 7's measurement build, still stands: the compile settings do not depend on it.
- `7e904e4` holds the redesign's tests, written before its change.
- The change, `plugins/declarations.rs`, stores each plugin's declaration in the cache under
  `declarations/<plugin sha256>/<entry>`, whose name is the digest of the entry format, the plugin
  interface and the core version. Every entry is written when its plugin loads and read only by
  that core. A missing or unreadable entry loads the plugin. `Loaded` keeps a declaration for every
  declared plugin and compiles only the plugins the run runs. The checks that need no plugin (one
  source per plugin, a pin per plugin, an exception's plugin declared where its file applies) run
  for every declared plugin. `gate cache prune` removes the declarations of plugins no lock pins.
- Overlap with T-056 (`gate cache prune`, Gate 004), decided by the lead: whichever branch merges
  second puts `declarations/` into T-056's derived class, like `results/<plugin sha256>/`. An
  unpinned plugin's directory goes; a pinned one's entries are bounded by `--older-than` and
  `--max-size`; `declarations::read` sets the entry's last use. The interim rule in
  `Cache::unreferenced` and its unit test then go.
- Measures, as the lead set them: after `gate sync` with the module cache emptied, plus one row
  for a first run on an empty cache, so the one-time cost is on record.
- Next: one round on the lane. It runs the new tests on `7e904e4`, each expected to fail but the
  two guards, then the gates on the tip.

# T-086: plugin load time, cold and warm

| Field | Value |
| --- | --- |
| Status | reviewed, round 429 green; waiting for the T-085 merge to rebase |
| Release | v0.3.0 |
| Writer | "Gate 002" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `6b36eeac` |
| Touches | `crates/gate-runtime/src/runtime.rs`, `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/lib.rs`, `crates/gate-host/src/cli.rs`, `crates/gate-host/src/doctor/mod.rs`, `crates/gate/src/main.rs`, their tests, `docs/guide`, `CHANGELOG.md` |

## Goal

Findings 2, 11 and 12 of the release-candidate trial (T-083):

- (2) A cold first run fails the hook budget: compiling the plugins counts toward `--warn-after` and
  `--fail-after`, and `gate doctor` says `ready` while nothing is compiled.
- (11) A warm run whose every call is answered from the result cache still loads every plugin.
- (12) `--timings` does not split `load` by plugin.

## How a check loads its plugins today

`plugins::load` (`crates/gate-host/src/plugins.rs`) runs before any call is planned:

1. `needed`: every plugin declared in a selected scope that `--rule` allows, whether or not its
   calls will come from the result cache.
2. `fetch_components`: for each, one after another, reads the blob from the cache and hashes it
   (the check that the bytes are the pinned ones).
3. `instantiate`: one thread per plugin; `Runtime::load` (`crates/gate-runtime/src/runtime.rs`)
   runs `Component::new` (wasmtime's module cache: on a hit it reads the file, zstd-decodes it and
   deserializes it; on a miss it compiles with Cranelift), `instantiate_pre`, a sha256 of the
   bytes, and the `describe` call; then the declaration is written to the cache, every run.

Only then are calls planned, and `calls` answers each from the result cache, keyed by the plugin's
sha256, the request and the grant. A cached call never touches the compiled module.

## Measurements

Built `gate` (release) and plugins from the T-076 lane round 394 (`main` + T-076); a probe build of
`main` at `1a32c105` with prints around each step of the load (never committed). Machine: 18 cores,
shared with the lane, so wall times move with its load; each number names the load average.
Commands: `gate check --timings` in a synthetic repository, a copy of the trial's `single` tree
and its cache, and one repository per plugin; the probe prints come with `GATE_LOAD_PROBE=1`.

Cold, one plugin per repository (nothing compiled; CPU is user + sys from `/usr/bin/time -p`):

| Plugin | wasm | compile CPU | warm `load` (3 runs) |
| --- | --- | --- | --- |
| agents | 2.2 MB | 2.38 s | 66, 31, 56 ms |
| ci | 1.8 MB | 1.89 s | 51, 27, 75 ms |
| docker | 0.5 MB | 0.79 s | 15, 45, 19 ms |
| docs | 0.6 MB | 0.80 s | 18, 15, 12 ms |
| git | 2.5 MB | 2.73 s | 282, 248, 31 ms |
| ledger | 2.2 MB | 2.43 s | 78, 110, 42 ms |
| markdown | 1.8 MB | 1.87 s | 92, 115, 23 ms |
| nestjs | 6.3 MB | 7.10 s | 1436, 1285, 554 ms |
| node | 1.5 MB | 1.89 s | 85, 97, 55 ms |
| repository | 1.5 MB | 1.49 s | 116, 160, 53 ms |
| typescript | 2.7 MB | 3.32 s | 234, 216, 180 ms |

(Load average 250 to 330 during that table; all eleven compile in 26.7 s of CPU.)

- Cold, nine plugins in one repository: `load` 5560 ms of a 5705 ms check (load average about 57);
  the same at load average about 250: 19.2 s. The trial measured 28.9 s. The compile runs on all
  cores at once, so its wall time is the slowest plugin's (nestjs) when the machine is free, and
  much more when it is not.
- Warm, nestjs alone (load average about 100), probe, five runs: `load` 95 to 142 ms, of which
  the module from wasmtime's cache 52 to 110 ms, `describe` 3 to 10 ms, the sha256 2 to 8 ms,
  read and verify 7 to 8 ms.
- Warm, the trial's `single` tree, nine plugins, every call `cached` (load average about 100),
  probe, three runs: `load` 659, 834 and 849 ms of 983, 1684 and 1228 ms in all. Read and verify of
  the nine blobs, one after another: 49 to 191 ms. The modules in parallel: nestjs 363 to 678 ms,
  typescript 29 to 116 ms, git 38 to 123 ms, the rest under 50 ms each. `describe` at most 16 ms.
  Module cache 9 hits, 0 misses.
- After a cold `gate sync`, the next `gate check` is warm: `load` 441 ms, no compile. `gate sync`
  already loads every declared plugin (`Needed::Every`), so it already compiles them.

So:

- (11) In a fully cached warm run the whole `load` phase, 0.66 to 0.85 s on the trial tree, is
  work no call uses: the parallel module phase 600 to 784 ms (bounded by nestjs's module, 363 to
  678 ms, and including writing declarations that are already stored), and reading and hashing the
  blobs, 49 to 191 ms.
- (2) In a cold run, `load` is 97 % of the run, and it is compile time.

## The fix

1. **Load a plugin only for a call that runs (11).** The calls are planned from the declarations,
   which the cache stores per plugin sha256 and core; a result is looked up by the plugin's sha256
   from `gate.lock`, the request and the grant, none of which needs the module. So `load` keeps,
   for each needed plugin, its sha256 and its declaration; the result cache is asked first; only
   the plugins with a call that misses are read, verified, compiled or taken from the module
   cache, and instantiated, once each, on the run's one runtime, in parallel. A plugin whose
   declaration is not stored still loads up front as today (it is needed to check the
   configuration), and `gate sync` still loads every plugin. Expected on the trial tree: `load`
   from 0.66 to 0.85 s to the declarations read, a few ms; to be measured.
2. **A stored declaration is not written again** on every load; it is written when it is new.
3. **Compiling does not count toward the budget (2).** `Runtime::load` reports per plugin how long
   its module took and whether the run's module cache missed (wasmtime counts hits and misses per
   cache, so the run knows exactly whether anything compiled, and how many). When a run compiled,
   the budget compares the run without its `load` phase, and the line says so: `gate/budget` then
   measures `the run took 280 ms, not counting 5560 ms compiling 9 plugins`, and the timings line
   says `load 5560 ms (compiled 9 plugins; not counted toward --warn-after and --fail-after)`.
   When nothing compiled, the budget is the whole run, as today.
   As built: the time left out is the wall time of each parallel load batch during which the
   runtime's miss count grew (the batch at the start, and each batch of on-call loads), never more
   than the run.
4. **Precompiling (2).** `gate sync` already compiles every declared plugin; no new command. `gate
   doctor` reports the plugins that are not compiled under this core (no stored declaration for
   the pinned sha256): `3 plugins are not compiled yet under this gate (nestjs, git, typescript);
   run gate sync before the first commit, or the first check compiles them`, instead of `ready`.
   The getting-started guide says to run `gate sync` after a clone, a core upgrade or a plugin
   bump.
5. **`--timings` per plugin (12).** One line per loaded plugin, `timings: load nestjs: module
   452 ms from the cache, describe 16 ms` (or `compiled in 7100 ms`), one line for the plugins not
   loaded, `timings: load: not loaded, every call cached: ci, docker, ...`, and the read and verify
   time of the blobs.
   As built: wasmtime counts hits and misses per runtime, not per module, and the modules of a
   batch load in parallel, so which plugin compiled is not knowable. The line per plugin is
   `timings: load <name>: module <ms> ms, describe <ms> ms`, and one summary line gives the counts:
   `timings: load: <n> read and verified in <ms> ms; modules: <n> from the cache, <n> compiled;
   <ms> ms compiling, not counted toward --warn-after and --fail-after` (the last part only when
   something compiled).

The lead's conditions on the design, and how the code meets them:

- Stored declarations are keyed by the pinned sha256, the core version and the plugin interface
  (`declarations::key`: the format, `gate_runtime::INTERFACE` and the core's version); after an
  upgrade of either there is no stored declaration, and the plugin loads up front.
- The cached-only path skips reading the blob, never the pin: `lock::verify_with` (gate.toml
  against gate.lock) runs before `plugins::load`, and `declared` fails on a plugin `gate.lock` does
  not pin, as before. A result is keyed by the sha256 `gate.lock` pins, so an edited sha256 finds
  no declaration and no result: the plugin loads up front from that pin and fails closed.
- Plugin-wide options are checked by the host against the options schema of the stored
  declaration, before any call, without loading the plugin; and they are part of the call's
  request (`plugin-options`), so a new value misses the result cache and loads the plugin.
- The budget message is as designed; the configuration page says that compiling does not count
  toward `--warn-after` and `--fail-after`, and so do the options' help texts.
- No `unsafe`, no Winch.

Not taken, measured or read:

- Our own compiled-module files read with `Component::deserialize_file` (memory-mapped, no zstd):
  wasmtime marks deserializing a precompiled module `unsafe`, and `unsafe` is forbidden here.
- Winch or a lower Cranelift optimization level for a faster compile: it changes how fast every
  plugin runs and was not measured; not for 0.3.0.
- Skipping `describe`: at most 16 ms, and it is the check that the bytes are the plugin they claim.

## Tests

1. A fully cached run loads no plugin
   (`a_run_whose_every_call_is_cached_loads_no_plugin_and_says_so`; these tests are in
   `crates/gate/tests/distribution.rs`):
   after a first run, the plugin's blob is removed from the cache; the next run with the same
   inputs passes from the result cache, downloads nothing, and `--timings` says `not loaded, every
   call cached: git`. The first run's `--timings` names the plugin loaded and the module compiled.
2. A warm cache serves nothing once `gate.lock` no longer pins the plugin
   (`a_warm_cache_serves_nothing_once_the_lock_no_longer_pins_the_plugin`), or once its sha256 is
   edited (`a_warm_cache_serves_nothing_once_the_pinned_sha256_is_edited`): both fail with exit 2
   and no report.
3. A plugin-wide option is checked without loading the plugin, and a new value misses the cache
   (`a_plugin_option_is_checked_without_loading_the_plugin_and_a_new_value_misses_the_cache`):
   with the blob removed and every call cached, `main = 5` is the configuration error
   `plugin git options: ...`; `main = []`, which the schema takes, misses and fails closed on the
   missing blob.
4. The budget (unit tests in `crates/gate-host/src/lib.rs`, so no test depends on how fast this
   machine compiles): a run that compiled is measured without its compile time and says so; a run
   that compiled nothing is measured whole.
5. `gate doctor` before `gate sync` names the plugin not compiled, and after it says it is
   compiled (`doctor_names_a_plugin_not_compiled_until_gate_sync_compiles_it`).
6. A stored declaration that does not parse is written again, and doctor does not count it
   as compiled (`a_stored_declaration_that_does_not_parse_is_written_again`).
7. A changed input still runs the plugin and stores the new result: the existing result-cache tests
   keep passing.

## Done when

- The tests above pass; the warm and cold measurements above are run again on the built tip, on
  the same trees, and the numbers are in this file with their commands.
- One lane round on the pushed tip on this task's own target directory: fmt, clippy, test, deny
  and the release build exit 0, and its log shows the changed crates compiled from this task's
  worktree.
- The leak scan prints nothing.

## Review fix round

The independent review of `1fa6135c` found one blocking item and five nits, all taken:

1. `with_generators`: the `.ok_or_else(` after `)?` is level with it, as rustfmt writes it.
2. A stored declaration that exists but does not parse is written again: the load writes when
   `declarations::read` does not give back what the plugin declares, and `stored` (doctor's
   `compiled`) asks the same, not whether a file exists. New test, item 6 above.
3. A `[generate]` plugin declared from the cache loads on the run's runtime with `load_on_call`;
   the other plugins are no longer loaded again on a new runtime for a `sync-state` answer.
   `PluginGenerators` finds a plugin loaded up front or on call.
4. `declarations::stored` and `declarations::write` each have their own doc comment.
5. Doctor's `compiled` uses the pins the `plugins` check verified instead of hashing every blob
   again; the configuration page says what compiled means.
6. `--timings` `not loaded, every call cached` names only plugins with a call planned; the others
   are left out.

## Lane rounds

- Round 415 (`9cd3b715`): fmt exit 0, clippy exit 101 (`too_many_lines` on `run` in `lib.rs`
  and `Loaded::run`, `unchecked_time_subtraction` in the budget); fixed in `03cba6ab`.
- Round 423 (`03cba6ab`, `T-086/lane/run-2.log`): fmt, clippy, deny, the release build and the 13
  wasm builds exit 0; the mutation (`OnCall` to `UpFront` in `gate check`) fails the cached-run
  test, exit 101, as it must. Test exit 101: three tests of `crates/gate/tests/plugins.rs` counted
  the modules a check compiles on the old rule that every needed plugin loads. Under this task a
  plugin with no call (spare declares typescript with no rule turned on) or whose calls come from
  the result cache is not loaded. `compiled_by` now empties the result cache too, and the two
  tests that expect typescript to compile turn on `typescript/parse` in spare, so it has a call;
  the expected counts are the ones they had.
- Round 429 (`d0eeb001`, `T-086/lane/run-3.log`): fmt, clippy, test, deny, the release build and
  the 13 wasm builds exit 0; the mutation fails the cached-run test, exit 101. `gate-runtime`,
  `gate-host` and `gate` compiled from `T-086/wt` (round 423 for the first two; only tests changed
  since).

## Measured on the built tip

The release `gate` of lane round 429 (`d0eeb001`; the code is that of `03cba6ab`, the commits
after it change tests and this file), with the same plugin builds and trees as above.
Commands: `gate check --timings` in the synthetic repository of nine pinned plugins (extending
the presets that exist: ci, docker, docs, git, node), one repository per plugin (`each.sh`), and
the copy of the trial's `single` tree and its cache; `/usr/bin/time -p` for CPU and wall time.
Load average 31 to 39 throughout (`uptime`, 06:53 to 06:54 UTC).

- Warm, the trial's `single` tree, nine plugins, every call `cached`, three runs after one
  warm-up run: `load` 4, 3 and 3 ms of 106, 95 and 84 ms in all (before: `load` 659 to 849 ms of
  983 to 1684 ms, at load average about 100). `--timings` says `not loaded, every call cached:
  ci, docker, docs, git, markdown, nestjs, node, repository, typescript`.
- Warm, the nine-plugin repository, three runs: `load` 3, 2 and 3 ms of 107, 97 and 109 ms; the
  four plugins with no rule turned on are not listed as not loaded.
- Warm, one plugin per repository, three runs each: `load` 1 ms for every plugin but nestjs, 3 ms
  for nestjs (before: 12 to 282 ms, nestjs 554 to 1436 ms, at load average 250 to 330).
- Cold, the nine-plugin repository: `load` 1417 ms of 1530 ms, 14.27 s of user CPU; `--timings`
  says `9 compiled; 1347 ms compiling, not counted toward --warn-after and --fail-after`, so
  `--warn-after 1` reports nothing. Again with `--warn-after 0.1` on a fresh cache: `warning
  [gate/budget]: the run took 287 ms, not counting 3072 ms compiling 9 plugins, over --warn-after
  100 ms`.
- Cold, one plugin per repository: compile CPU 0.57 s (docs) to 5.57 s (nestjs), `load` 55 to
  530 ms.
- `gate doctor` before any run: `fix  compiled: 9 plugins are not compiled yet under gate 0.2.1
  (...)`. On a fresh cache, `gate sync` took 1.56 s; then doctor says `ok   compiled`, and the
  next check loads the five plugins with a call from the module cache: `modules: 5 from the
  cache, 0 compiled`, `load` 2 ms.
- `describe` prints as 0 ms: under a millisecond at this load (the line rounds down).

## Resume point

2026-11-21T06:55:08+00:00: lane round 429 green on `d0eeb001` (`T-086/lane/run-3.log`),
measurements on the built tip in this file. Next: the T-085 rebase when the lead says so,
`Loading::UpFront` in its `load` call, one round.

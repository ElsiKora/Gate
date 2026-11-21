# T-086: plugin load time, cold and warm

| Field | Value |
| --- | --- |
| Status | code written, for the lane round and an independent review |
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
6. A changed input still runs the plugin and stores the new result: the existing result-cache tests
   keep passing.

## Done when

- The tests above pass; the warm and cold measurements above are run again on the built tip, on
  the same trees, and the numbers are in this file with their commands.
- One lane round on the pushed tip on this task's own target directory: fmt, clippy, test, deny
  and the release build exit 0, and its log shows the changed crates compiled from this task's
  worktree.
- The leak scan prints nothing.

## Resume point

2026-11-21T02:51:31+00:00: the design approved with the lead's conditions; code, tests, the
configuration page, the getting-started guide and the changelog written, not compiled yet. Next:
commit, one lane round, then the measurements again on the built tip.

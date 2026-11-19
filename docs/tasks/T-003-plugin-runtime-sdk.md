# T-003: Plugin interface, runtime, SDK and fixture harness

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 002" |
| Reviewer | a separate review session |
| Depends on | T-001 (form decided), T-002 (host API; read its branch, rebase when it merges) |
| Touches | `wit/**`, `crates/gate-runtime/**`, `crates/gate-sdk/**`, `crates/gate-test/**`, `examples/plugin-hello/**`, root `Cargo.toml` members and `Cargo.lock`, `deny.toml` (licences of wasmtime's tree) |

## Goal

A plugin author writes rules in Rust against `gate-sdk`, builds one `.wasm` component, tests it with
`gate-test` against fixture trees, and the core runs it through `gate-runtime` with the sandbox,
read-set recording and module cache of [architecture.md](../architecture.md) sections 3, 4 and 9.

## Scope

1. **First, within two hours, push the SDK surface** (types and traits, bodies may be `todo!()` on the
   branch only): `wit/gate.wit` (`gate:plugin@0.1.0`, as section 4 with the fields settled in sections
   7 and 16: `finding.target`, `rule-run.options-file`, `rule-run.exceptions`, `check-request.plugin-options`,
   `languages`, `norms`, `generate-result`, `git.sync-drift`), and `gate-sdk`'s public API: `Tree`
   (`paths`, `under`, `text`, `bytes`, `changed`), `Git`, `Report::finding(aspect, location, message)`
   with `target`/`help` builders, typed options via `serde`, rule and generator declaration (a
   `#[gate::plugin]`-style macro or a plain registration function: your choice, simplest that reads
   well). Message "Gate Lead" when it is pushed: four porting tasks start from it.
2. `gate-runtime`: embed `wasmtime` (latest stable on crates.io; the spike used 50.0.1), component
   model, WASI 0.2 with no preopened directories, no network, no clock (deterministic stubs), no
   environment; host implementations of `tree`, `git`, `log` over `gate-host`'s `Snapshot` and git
   module (T-002 API); grants: project subtree, `reads` paths, `git` only for rules that declare
   `reads-history`; read-set recording (paths listed, paths read with blob ids, git queries); the
   compiled-module cache keyed by (sha256, wasmtime version, target triple), atomic writes; fuel or
   epoch limit so a runaway plugin fails with a clear message; parallel calls over a worker pool.
3. Wire into `gate check` in `gate-host`/`gate`: load plugins from local paths for now (`--plugin
   name=path` and a `path` resolution used by tests); T-004 adds fetching by lock. Options validated
   against each rule's JSON Schema (`jsonschema` crate or equivalent, pinned).
4. `gate-test`: run a plugin `.wasm` (or the rule code natively through the same SDK, if that is
   simpler and exact) against a fixture directory and an expected-findings file; a `bless` mode.
5. `examples/plugin-hello`: a minimal plugin with one rule, its fixtures and test, built in CI; used by
   the guide.

Not in scope: porting real rules; fetching and lock (T-004); the result cache across runs beyond the
read-set record (store and lookup land here if cheap, else T-004).

## Done when

- fmt, clippy, test, deny exit 0 (wasmtime builds go through the queue).
- Tests: a plugin that tries to read an ungranted path gets `none`; a rule without `reads-history` cannot
  call `git`; options failing the schema fail the run with a message naming file and key; a plugin
  trap and a fuel exhaustion fail with the plugin name; the read set of a call equals what it read; the
  module cache is hit on the second run (count compiles).
- `gate check` on a fixture repository with `examples/plugin-hello` reports its finding with norm,
  level and target from configuration.
- Leak scan prints nothing.

## Notes

- T-001's spike code (`spike/` on branch `agent/t-001-plugin-form-spike-f4a648`) has a working
  wasmtime host and guest: reuse what is good.
- The SDK is what every porting task writes against: keep it small, documented, and close to the shape
  the reference implementation's checks used (`Snapshot::paths/under/text`, `Report`), so ports are
  moves, not rewrites.

## Merge with T-005

Whichever of T-003 and T-005 merges second rebases and wires these:

- `--target-ref <ref>`: T-005 owns the flag on `check` and `run`, valid only with `--push` and carried
  in the change selection as `Option<String>` (Gate 003 names the field). T-003's own flag
  (`gate check --target-ref`, `ChangeRequest::Push { target_ref }`) is dropped then; T-003
  only feeds T-005's value into `commit-range.target-ref` through `PushRange::target_ref`, and keeps
  the test in `crates/gate/tests/plugins.rs` (a history rule sees the ref with the flag, none
  without it) against T-005's flag. The value is always a full `refs/...` ref: T-005's CLI refuses
  anything else and its generated CI passes `refs/heads/${{ github.base_ref }}` or `github.ref`, so
  T-003 does no format handling of its own. T-005 merges first.
- `Generators::reads_history(&RuleFilter)`: T-005 answers false until the runtime lands; T-003
  answers it from the loaded plugins' rule declarations (`reads-history`), so generated CI gives
  history jobs `fetch-depth: 0`.

## Resume point

2026-11-17T01:52:37+00:00: merged to main; gates on the merged tree all exit 0. The
review of the fix round found no blocker; its findings are T-013.

2026-11-17T00:43:08+00:00: origin/main with T-005 merged in (a merge, not a rebase, so the reviewed
commits keep their ids). `--target-ref` is T-005's flag; `PushRange::target_ref` comes from
`ChangeRequest::Push`. `PluginGenerators` answers T-005's `Generators`: `generate`, `owns` and
`reads_history` from the loaded plugins. `gate sync` reports tracked files under an owned glob that
no generator writes, and removes them through T-005's guard. Generator findings are not surfaced yet.
`git.sync-drift` answers what `gate sync --check` reports at the commit, with the generators its
`gate.lock` pins (or the run's `--plugin` builds), stored in the cache's `git/sync-drift/` by commit,
core version and builds. Before the move to Gate (a `gate.toml` this core does not read) it uses the
run's configuration on the commit's tree, keyed also by that configuration's hash (architecture);
a failing generator answers its owned files and the paths it writes in the run. Next: the
full round on the tip.

2026-11-16T22:21:36+00:00: items 1, 2, 3 and 5 written and pushed; `gate-runtime`, `gate-host` and `gate` have
not been compiled yet (every wasmtime build goes through the queue; the job builds a pinned commit
in its own checkout: runtime and SDK tests, workspace clippy, workspace tests). `gate-sdk` and
`examples/plugin-hello` are checked directly: clippy native and wasm32-wasip2, tests. `gate-runtime`:
wasmtime 50.0.1 with the component model and the fixed WASI stubs; grants (`../` paths outside the
root, findings and read sets normalized to repository paths, a commit location passed through);
read-set recording, with the ids each ref answer (head, branch, range, resolve, log ranges)
depended on; the module cache in `Cache::modules()`; fuel (10^12) and memory (4 GiB) limits set by
the core, failing as `OutOfFuel` or `OutOfMemory` naming the plugin and the limit; a worker pool.
Its tests run the probe plugin in `crates/gate-runtime/tests/probe`. Item 3: `gate-host::plugins`
loads each declared plugin from its `--plugin` override or its `gate.lock` pin through T-004's
fetcher and cache, checks the bytes' checksum, validates options with `jsonschema` 0.61.2, runs
enabled rules (`[rules]`, family levels, presets) per selected scope with the run's commit range,
fills `RulesRan`, and returns the read sets in `Checked::reads`. Item 5: `examples/plugin-hello`
(`hello/marker`), its fixture test, and `crates/gate/tests/plugins.rs`, which runs it pinned from
the cache and by override (finding with norm, level and target; options failing the schema; the
override line; a pin that cannot be fetched; cached bytes that differ). Item 4: `crates/gate-test` runs a rule natively on fixture cases (`tree/`, `options.json`,
`expected.txt`) and compares, or writes `expected.txt` under `GATE_BLESS=1`; plugin-hello's fixture
test uses it. `rust-toolchain.toml` lists `wasm32-wasip2`, which the probe and hello builds need. Not done: `--fail-fast` inside plugin calls, a persistent cache of tree and merge-tree
answers by commit id. Next: fix what the queued build finds.

# T-019: A schema for `[plugins.<name>] options`, checked at load

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.1.0 |
| Writer | "Gate 002" |
| Reviewer | a separate review session |
| Depends on | T-014 (rebased on `task/T-014`, main) |
| Touches | `wit/gate.wit`, `crates/gate-sdk/src/{plugin,bindings,lib}.rs`, `crates/gate-runtime/src/runtime.rs`, the plugin load path in `crates/gate-host/src/plugins.rs`, `crates/gate-host/src/workspace.rs`, every plugin's `lib.rs` and the settings types of `git`, `ledger` and `nestjs`, the probe, their tests, docs/architecture.md §4 and §7 |

## Goal

The plugin interface declares the JSON Schema of a plugin's own options, `[plugins.<name>] options`
(or `options-file`), as it already does for each rule and each generator. The host checks those
options against it when the plugins load: a missing required option, an unknown option or a value of
the wrong type is a configuration error, exit 2, naming the file, line and plugin, before anything
runs.

## Why

On main (lead, docs/plan.md "From the first consumer's move"): the interface declares
schemas per rule and per generator but none for a plugin's own options. A missing `[plugins.git]
options main` surfaces only when a git rule runs, as an environment error from the plugin. The
interface is not public yet (v0.1.0), so it changes now, with no compatibility path.

## Scope

1. WIT: `plugin-info` gains `options-schema: string`, the JSON Schema of `[plugins.<name>] options`.
2. SDK: `plugin!` takes `options: <type>` (a `Deserialize + JsonSchema` type), and `describe` answers
   its schema. A plugin without options of its own names `gate_sdk::NoOptions`, which accepts only an
   empty table. One form: the field is required.
3. Runtime: the schema travels from `describe` into `PluginInfo`.
4. Host: when a component loads, its options schema is parsed like a rule's (a schema that is not JSON
   is a broken plugin). Validation then checks the options of every declared plugin entry, in the root
   and in each project, against it; absent options are checked as `{}`, so a required option is
   reported missing. A plugin that did not load is not checked.
5. Plugins: `git` declares its `Settings` (`main`, `activation` and `release-subject` required),
   `ledger` and `nestjs` their option types; `agents`, `ci`, `markdown`, `typescript` and the probe
   declare `NoOptions`, except that the probe declares an optional `label` so the end-to-end tests
   have options to break.
6. Architecture §4: `plugin-info` in the sketch; §7: plugin options among what is validated at load,
   and the root example's `[plugins.git] options` valid under the git plugin's schema.

## Tests

- Host, configuration level (`crates/gate-host/tests/host/config.rs`): a missing required plugin
  option, an unknown plugin option and a plugin option of the wrong type are configuration errors
  naming the file, line and plugin; a valid one passes; a project's entry is checked too.
- End to end (`crates/gate/tests/plugins.rs`, the probe): `gate check` with an unknown option and with
  a wrong type exits 2 and runs nothing; a valid option passes.
- Git plugin: its declared options schema requires `main`, `activation` and `release-subject` and
  refuses an unknown key.

## Done when

- The tests above.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0, run on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-18: written and pushed; nothing that compiles wasmtime has run yet.
Native clippy (`-D warnings`) and tests of `gate-sdk`, `gate-test` and the plugins pass (464
passed, 0 failed), and wasm32-wasip2 clippy of the probe passes, all with
`CARGO_TARGET_DIR` set to this task's own SDK target directory. Next: the gates on the pushed
commit.

- Plugin options are checked before rule options, so a broken `[plugins.<name>] options` is the
  first error reported.
- docs/guide/writing-a-plugin.md: the `plugin!` example and its text carry the `options` field.

Review round (lead, 2026-11-18, review: merge after fixes), rebased on `task/T-014`:

1. `schema()` compiles every options schema, the plugin's and each rule's, with
   `jsonschema::validator_for` when the component loads. A schema that is not JSON Schema (`{"type":
   5}`) makes the plugin broken for good: unloadable on the history path, not a fallback.
2. An empty options schema is a broken plugin too, so it is not a second form of "accepts anything".
   The gate-test doc example declares `gate_sdk::bindings::schema::<gate_sdk::NoOptions>`.
3. The interface keeps `gate:plugin@0.1.0`. A component whose imports or exports do not match this
   core's world fails to load with "<plugin> was built for another plugin interface; rebuild it or
   pin a release built for gate <version>". It is tested with an empty component, which fails the same
   export check a component of the earlier `plugin-info` fails.
4. Plugin options are checked wherever the plugin is declared, whatever its rules' levels. A past
   commit whose plugin options fail the schema falls back to the run's configuration, like any other
   configuration error. Architecture §7 and the `sync-state` paragraph say both, and each has a test.
5. Tests: an options schema that is empty, not JSON or not JSON Schema fails to load; the
   missing-git-option test asserts empty stdout; the unit test covers a root entry.
6. Docs, on main's public-pass text (lead: take main's text, re-apply the git options):
   every guide and README example that declares git carries `main`, `activation` and
   `release-subject` in `[plugins.git.options]` and turns on `git/no-ci-skip` instead of the rule
   ids the plugin does not have. configuration.md says the options are validated at load. One such
   rule id, `git/branch-name`, is left in CONTRIBUTING.md, which this round does not cover.

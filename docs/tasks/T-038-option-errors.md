# T-038: A rule's option error is a configuration error

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | T-036 (`task/T-036-commit-message-options` at `c8afefb`, for `plugins/git/src/javascript.rs`); merges right after T-036 |
| Touches | `wit/gate.wit`, `crates/gate-sdk`, `crates/gate-runtime`, `crates/gate-host`, every plugin's option-validation sites, `CHANGELOG.md` |

## Goal

A plugin can tell the core that the configuration it was given is wrong, apart from an environment
failure. A bad rule or generator option then ends the run as a configuration error (exit 2, the
label `configuration error`), as a bad `gate.toml` key does, and not as an environment error.

## Scope

Step one (pushed as soon as it compiles):

1. WIT: `check` and `generate` answer `result<_, plugin-error>`, with
   `variant plugin-error { configuration(string), environment(string) }`.
2. SDK: `Error::config` beside `Error::new`; options that do not match a rule's or generator's
   options type, and `[plugins]` options or `[languages]` settings that do not match, are
   configuration errors inside the SDK. The kind passes through the `rule ... failed` wrapping.
3. Runtime: a new `ErrorKind::Configuration` for an answered `configuration`; it recurs for the same
   input, so it never marks a failure as one that may pass. Host: a call that fails with it is a
   configuration error.
4. The git plugin swept: only option-validation sites move to `Error::config`; read, parse and git
   failures stay environment errors. `git/branch-name` reads its patterns through
   `plugins/git/src/javascript.rs`, as `git/commit-message` does, and `ascii_classes` goes, with a
   test per silent case `git/branch-name` had and a line on its rule page.
5. Tests: a bad option in two plugins gives exit 2 and the configuration-error label; a missing
   plugin file keeps its environment-error label.

Step two: the other plugins swept site by site, by the same rule.

`CHANGELOG.md`: `Changed`, the WIT and the SDK. No parsing of message conventions.

## Moved sites

Each `Error::new` site moved to `Error::config`, by file:line at the commit that moves it.

Step one:

- SDK: `crates/gate-sdk/src/plugin.rs:258` (options that do not match the options type),
  `crates/gate-sdk/src/rule.rs:209` (`[plugins]` options), `crates/gate-sdk/src/rule.rs:221`
  (`[languages]` settings).
- `plugins/git/src/settings.rs`: 50 (the `[plugins.git]` options do not match), 55 (`main` empty),
  66 (`activation` not a full id), 76 (`release-subject` without a group), 85 (`branch-length`
  inverted), 99 (no `activation`), 220 (a required key missing), 232 (a pattern that does not
  compile), 250 and 256 (globs).
- `plugins/git/src/forms.rs`: 45, 198, 208, 214, 228, 238, 246 (forms that cannot work).
- `plugins/git/src/commit_message.rs`: 250 and 256 (`header-pattern` and `ignores` refused or not
  compiling), 270 (no `type` group).
- `plugins/git/src/branch_name.rs`: 166 (`min-length` above `max-length`), 187 (template and
  pattern), 287 (a template with nothing to match), 426 and 432 (a pattern refused or not
  compiling).
- `examples/plugin-hello/src/lib.rs:34` (`marker` empty), the second plugin of the end-to-end test.

Kept as environment errors: `plugins/git/src/history.rs:76` (no main reference exists) and
`plugins/git/src/history.rs:83` (the activation commit is not an ancestor of main), since a fetch of
the full history may fix either; the SDK's `are not JSON` and `has no rule` (what the core sent,
not what the user wrote).

## Step two: nestjs and ledger (Gate 005, branch `task/T-038-sweep-nestjs-ledger`)

Each `gate_sdk::Error::new` site, by file:line at the commit that moves it; the test for each is
whether editing `gate.toml` or a file the options name fixes the error. `SqlError::new` in
`advisory_lock_boundary/sql.rs` is the SQL reader's own error type, not an SDK error; it is not a
site.

`plugins/nestjs/src` (22 moved, 3 kept):

- Moved: `shared/paths.rs` 57, 60, 63, 66 (a path or name option not in normal form).
- Moved: `shared/pattern.rs:11` (a pattern option that is no regular expression; its two callers
  that pass a fixed pattern cannot fail).
- Moved: `rules/semantics/native_functions.rs:156` (`function-decorators` empty).
- Moved: `rules/semantics/prefix_nesting.rs:73` (`word-separator` empty).
- Moved: `rules/semantics/entity_table_name.rs:56` (`decorator` empty).
- Moved: `rules/semantics/package.rs:14` (a package option empty).
- Moved: `rules/semantics/folders.rs:12` (a folder option that is the project root).
- Moved: `rules/semantics/wrappers/mod.rs:312` (the `package-exports-file` the options name is not
  tracked) and 317 (that file's table cannot be read).
- Moved: `rules/semantics/module_registry/mod.rs:113` (`table-header` not three cells).
- Moved: `rules/semantics/owner_boundaries/mod.rs:149` (a package option with an empty package or
  name).
- Moved: `rules/structure/import_direction.rs:133` (`contracts` empty).
- Moved: `rules/structure/notification_egress/mod.rs:228` (the decision index `decisions` names is
  not tracked) and 284 (`invalid`, the helper of every check of the options and of the decision
  index's accepted decisions).
- Moved: `rules/structure/runtime_factory.rs:57` (`contracts` empty).
- Moved: `rules/semantics/typeorm_relation/mod.rs:242` (`foreign-key-case` without
  `foreign-key-label`), 251 (`resolved-aliases` names an alias the layout does not define) and 494
  (a join reaches a foreign key name and `foreign-key-case` is not set).
- Moved: `rules/structure/raw_config_access.rs:92` (`raw-imports` empty).
- Kept: `shared/project.rs:30` (a `gate_typescript` library error: a source file that cannot be read,
  a tsconfig that cannot be parsed).
- Kept: `rules/structure/package_pins.rs:130` (the npm manifest or lockfile is not JSON: a tracked
  source file, not an option).
- Kept: `rules/structure/advisory_lock_boundary/source/shell.rs:39` (the plugin's own fixed pattern;
  a failure is a defect of the plugin).

Test: `crates/gate/tests/plugins.rs`, `a_nestjs_option_its_rule_refuses_is_a_configuration_error`:
`nestjs/eslint-directives` with `rule-id-pattern = "("` gives exit 2 and `gate: configuration
error:`.

## Done when

- fmt, clippy, test (`--no-fail-fast`), deny and build exit 0 in one lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-038-option-errors` from T-036's tip `c8afefb`; this file first. No round
until T-036's ticket 187 is green.
2026-11-19: step one written: `plugin-error` in the WIT, `Error::config` in the SDK,
`ErrorKind::Configuration` in the runtime, the host's label; the git plugin and the hello example
swept; `git/branch-name` on `javascript.rs` with `ascii_classes` gone, eleven per-case tests and a
line on its page; the end-to-end test of hello's and git's bad options (exit 2, `configuration
error`) and a missing `--plugin` file (`environment error`). Waiting for T-036's ticket 187 before
a round.

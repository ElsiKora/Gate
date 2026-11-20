# T-071b: The workspace preset test after T-057

| Field | Value |
| --- | --- |
| Status | test fixed and the bug it found fixed; the round runs after the Gate 001 stack merges |
| Release | v0.3.0 (a host fix and its tests) |
| Writer | "Gate 003: host foundations" |
| Reviewer | the lead |
| Depends on | `main` at `2ef2cc61` (T-071, T-057 and the Gate 001 stack merged) |
| Touches | `crates/gate/tests/plugins.rs`, `crates/gate-host/src/{workspace,plugins}.rs`, `crates/gate-host/tests/host/config.rs`, `CHANGELOG.md` |

## Goal

The integration round on `main` at `308eca9b` failed one test: T-071's
`a_monorepo_root_extends_git_recommended_and_its_own_rules_win` expected `error[git/branch-name`
from a `gate check --hook --commit-message` run. T-057, merged after it, makes a
`--commit-message` check run only the rules that read the message, so `git/branch-name` no longer
runs there. That is T-057's design, not a bug; the test's assumption is what changed.

## Change

The test keeps its intent, that `[workspace] extends = ["git/recommended"]` at a root that is not a
project turns on both repository-scope rules with the preset's aspect levels and that the root's
`[rules]` wins, and proves each rule through a run that judges it:

- `git/branch-name` through `gate check --hook --rule git/branch-name`: `error` from the preset,
  then `warning` from the root's `[rules]` (exit 1, then 0);
- `git/commit-message` through `gate check --hook --commit-message`: `header-min-length` at
  `warning` and `subject-min-length` at `error` from the preset, `note` for both from a level of
  the whole rule, and an aspect level of the root's over both; the run reports no
  `git/branch-name`, as T-057 has it.

## The bug the test found

Round 384 on `79916e4a` failed the test's second case with exit 2: `gate.toml:13 [rules]:
"git/branch-name" options: {} is not valid under any of the schemas listed in the 'oneOf'
keyword`. A `[rules]` entry that sets only a level for a rule a preset turns on was checked as if
its options were `{}`, although the run uses the options the preset gives the rule
(`enabled_rules` falls back to them). Only `off` escaped the check, which is why the test on `main`,
which turned `git/branch-name` off, never met it. The same held in a project's file and a profile.

By the lead's ruling the fix is part of this task: `check_rule` validates the options the run uses,
the entry's own, else what the scope's presets give the rule, else `{}`. `Workspace::validate` and
`validate_except` take the presets' answer as a closure, which the plugin runtime builds from the
presets each scope extends.

The test `a_level_in_rules_keeps_the_options_a_preset_gives_the_rule` holds the project and profile
cases with the probe's preset `probe/needs`, which gives its rule the required `pattern`: a level
alone runs with the preset's pattern, the entry's own options replace it, and without the preset a
level alone is still a configuration error that names the missing option.

## The review, one fix round

The independent review of `5e3fc9c7` found nothing blocking and asked for three additions:

1. A test that the entry's own options are checked even when the preset's would pass: a project
   entry `{ level = "warning", options = {} }` over `probe/needs` exits 2 at the entry.
2. A test that a later preset replaces an earlier one's settings, options included:
   `extends = ["probe/needs", "probe/needs-level"]` with a level alone exits 2.
3. When the options checked came from a preset, the error says so: an environment error at the
   `extends` entry, `"<rule>" options the preset <plugin>/<preset> gives: ...; the plugin is broken,
   report it to its authors`, not the user's `[rules]` line. The probe gains the preset
   `needs-broken`, which gives `pattern` a number, and the test
   `options_a_preset_gives_that_break_the_schema_are_the_plugins_error` holds the message.

`Workspace::validate` and `validate_except` receive a `PresetOptions` (the options and the
`extends` entry of the latest preset that sets the rule) instead of a bare value.

## Done when

- One lane round on the pushed tip, its own target directory and its logs kept per ticket: fmt,
  clippy, test, deny and the release build exit 0; the log shows `gate` compiled from this task's
  round checkout and lists both tests by name; the workspace test fails with `[workspace] extends`
  not layered into the root's settings, and both fail with the presets' options left out of the
  check.

## Resume point

2026-11-20T23:23:48+00:00: rebased onto `main` at `2ef2cc61` (the Gate 001 stack merged), with no
conflict in `workspace.rs`; pushed, and the lane round is queued.

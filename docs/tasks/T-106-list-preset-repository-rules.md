# T-106: `gate list` shows the repository-scope rules a preset turns on

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | the next core release |
| Writer | "Gate T-106" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `80db512` |
| Touches | `crates/gate-host/src/` (the `list` command and the rule set it prints), its tests, `docs/plan.md`, this task file |

## Goal

A configuration extends `ci/recommended` and `git/recommended` and writes no `[rules]` entry for
`ci/permissions`, `ci/runner-pinned`, `git/branch-name` or `git/commit-message`.

`gate check` runs those rules and reports their findings, but `gate list` does not print them. A
`gate check --rule ci/permissions` probe finds them. So the list a user reads to know what runs is
wrong for repository-scope rules that only a preset turns on. Seen on the first consumer with
v0.3.2.

## Required outcome

1. `gate list` prints every rule that `gate check` would run, in every scope, at the level the
   check resolves. A rule that a preset turns on is marked with that preset, as project-scope
   rules are today.
2. The JSON form of `gate list`, if any, matches the text form.
3. `gate list` and `gate check` resolve the rule set from one shared function, so the two cannot
   drift. If they already do, find why the repository scope differs.

## Proof

- A test on a synthetic repository that extends a preset turning on repository-scope rules,
  with no `[rules]` entries: `gate list` shows them with the preset mark, and the set equals the
  rules a `gate check` records as run.
- A rule that a `[rules]` entry sets `off` is not listed as running.
- All cargo runs go through the lane.

## Cause

`gate check` runs the repository scope's rules from the root `gate.toml` even when the top is a
project. `gate list` prints no repository section at such a top (the root project's section stands
for it), and that section took only the project-scope rules its presets turn on: the
repository-scope ones (`ci/*`, `git/branch-name`, `git/commit-message`) fell between the two. The
JSON form printed them under `repository` but not under the root project. The preset rows also came
from their own walk of the presets, so a preset's rule that `[rules]` turned off was still listed.

## Change

- `Loaded::enabled` (gate-host `plugins.rs`) is the one walk of every scope's enabled rules by
  plugin. The check run, `--skip-rule`'s `skipped-rules`, the options-needs check and
  `preset_settings` (what `gate list` and `gate explain` print) all take it.
- `preset_settings` gives a row only to a rule that walk turns on, so an `off` rule has none.
- `gate list` (text and JSON) and `gate explain` give the root project's section the repository
  scope's preset rows before its own.

## Resume point

- Red: the new test
  `list_prints_the_repository_rules_a_root_projects_presets_turn_on_as_check_runs_them`
  (`crates/gate/tests/plugins.rs`) failed on `052a48fd`'s code through the lane
  (`one.sh list_prints_the_repository_rules`, exit 101: "ci/permissions from ci/recommended: {}").
- Fix written; lane runs of clippy, the test and the full set pending.

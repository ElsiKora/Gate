# T-106: `gate list` shows the repository-scope rules a preset turns on

| Field | Value |
| --- | --- |
| Status | in review |
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
  (`crates/gate/tests/plugins.rs`) failed on `052a48fd`'s code through the lane, exit 101
  ("ci/permissions from ci/recommended: {}": no preset rule listed for the root project, in text
  or JSON).
- `dc2483fe` did not compile (`enabled` in the wrong impl; clippy exit 101); `7ed7c985` fixes it.
- Lane round on `7ed7c985`'s tree (`round.sh`, 2026-11-26T06:22:04+00:00 to 06:42:44+00:00):
  `cargo fmt --all --check` exit 0, `cargo clippy --locked --all-targets -- -D warnings` exit 0,
  `cargo test --locked` exit 0 (1654 passed, 0 failed, the new test and the existing list,
  explain and preset tests among them), `cargo deny --locked check` exit 0.
- Waiting for review. Commits after `7ed7c985` touch this file and `docs/plan.md` only.

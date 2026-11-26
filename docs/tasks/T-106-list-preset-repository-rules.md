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
  plugin. The check run, `--skip-rule`'s `skipped-rules` and the options-needs check take it.
- `preset_settings` gives every preset row of each scope, a preset that sets a rule `off`
  included, and marks a row `effective` when no `[rules]` entry or family level of the scope
  overrides it (the precedence of a run: `level`, `Settings::setting`, `Settings::options`).
- `gate list` (text and JSON) shows the effective rows: for each rule, its `[rules]` entry or
  family, else its preset row. A rule listed at a level other than `off` is one check runs.
  `gate explain` shows every row, overridden ones included, as on `main`.
- The root project's section takes the repository scope's preset rows too, ordered by rule.

## Review round

The review (kept outside the repository) found nothing
blocking. The lead's decisions, all done in `c04fed22`:

1. S1 and S2: `explain` shows every row again; a rule only a preset sets `off` is listed as
   `off (preset <p>)` and explained.
2. `list` shows per rule the row that gives its effective level.
3. N1: the tests compare `list` with the findings a check reports, on the probe plugin at a root
   project: presets on, a preset that sets a rule `off` (new probe preset `whole-off`), a `[rules]`
   override, and a family set `off`.
4. `docs/guide/plugins.md` and the configuration guide's `list` paragraph updated;
   `docs/guide/writing-a-rule.md` still matches.
5. N3 fell out: the root project's preset rows are one run ordered by rule.

## Resume point

- Red: the first test (now `list_prints_the_repository_rules_a_root_projects_presets_turn_on`,
  `crates/gate/tests/plugins.rs`) failed on `052a48fd`'s code through the lane, exit 101
  ("ci/permissions from ci/recommended: {}": no preset rule listed for the root project, in text
  or JSON).
- First round green on `7ed7c985` (fmt, clippy, 1654 tests, deny: each exit 0).
- Review round: `origin/main` (`2483e35b`, T-104) merged in `703e40ca`; the fix is `c04fed22`.
  Lane: clippy alone, then the full round, results below.

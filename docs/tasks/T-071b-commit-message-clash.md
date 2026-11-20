# T-071b: The workspace preset test after T-057

| Field | Value |
| --- | --- |
| Status | test fixed; lane round next |
| Release | v0.3.0 (test only; no product change) |
| Writer | "Gate 003: host foundations" |
| Reviewer | the lead |
| Depends on | `main` at `308eca9b` (T-071 and T-057 merged) |
| Touches | `crates/gate/tests/plugins.rs` (`a_monorepo_root_extends_git_recommended_and_its_own_rules_win`) |

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

No product code changes.

## Done when

- One lane round on the pushed tip, its own target directory and its logs kept per ticket: fmt,
  clippy, test, deny and the release build exit 0; the log shows `gate` compiled from this task's
  round checkout and lists the test by name; the test fails with `[workspace] extends` not layered
  into the root's settings.

## Resume point

2026-11-20T21:12:10+00:00: the test is fixed and pushed; the lane round is next.

# T-036: header-pattern, ignores and default-ignores of git/commit-message

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | `release/v0.2.0`; merges before T-035, which stays last |
| Touches | `plugins/git/src/commit_message.rs`, its tests, `plugins/git/rules/commit-message.md`, `plugins/git/CHANGELOG.md` |

## Goal

A repository whose commit policy is a header regular expression and its own list of messages to
skip can write it as options of `git/commit-message`, at parity with commitlint's `headerPattern`
(of `conventional-commits-parser`), `ignores` and `defaultIgnores`, instead of a shell step.

## Scope

1. `header-pattern`: one regular expression with the named groups `type`, `scope` and `subject`, read
   as JavaScript reads one without flags (ASCII `\w`, `\d`, `\b`). Unset, the header is read as today,
   with the conventional pattern. The breaking header (`type(scope)!: subject`) stays the parser's one
   path, tried first as the preset tries it. A pattern that does not compile, or has no `type`
   group, is a configuration error that names the option and says what to write.
2. `ignores`: regular expressions; a message one matches is not checked. They are added to the
   default ignores.
3. `default-ignores = false`: commitlint's default ignores (merges, reverts, reapplies, `amend!`,
   `fixup!`, `squash!`, a release whose header is a version) no longer apply.
4. The rule page and the plugin's changelog (`Added`), with synthetic examples.

## Done when

- A test writes 30 synthetic messages of a header-regex policy (ASCII scope, a type list, merges and
  reverts skipped through `ignores` with `default-ignores = false`) and each gives the verdict the
  policy's regex gives.
- A pattern that does not compile, and one with no `type` group, fail closed with a configuration
  error naming `header-pattern`.
- With `default-ignores = false`, `fixup! x` is checked and fails.
- fmt, clippy, test (`--no-fail-fast`), deny and build exit 0 in one lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-036-commit-message-options` from `release/v0.2.0` at `264264f`; this file
first.

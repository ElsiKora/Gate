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
   path, tried first as the preset tries it, unless `header-pattern` is set: then that one pattern
   decides the header, a breaking `!` included (the lead's decision). A pattern that does not
   compile, or has no `type`
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
2026-11-19: written. `header-pattern` (named groups, JavaScript reading: ASCII classes, `.` without
line terminators), `ignores` and `default-ignores` in `plugins/git/src/commit_message.rs`; the
breaking form is still read with the conventional breaking pattern first, as commitlint's preset
reads it, so a custom pattern's scope does not apply to `type(scope)!:` headers. Tests: the 30
messages of a header-regex policy, a pattern that does not compile or has no `type` group, an
ignore that does not compile, `default-ignores = false` with `fixup! x`, the JavaScript reading.
An option error reaches the user as exit 2 labelled "environment error", as every rule's option
error does today (branch-name, main-history): the host labels a rule's error, not the rule. No
native cargo run; the lane round runs the gates.
2026-11-19: lead's decision: a custom `header-pattern` replaces the breaking path too; the default
keeps the breaking pattern first. The policy test now expects `feat(a)!: add x` to pass and
`feat(a_b)!: add x` to fail; ticket 175 had not started, so it builds this tip.

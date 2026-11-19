# T-055: git/commit-message checks the pushed commits too

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | T-042 (`prohibited-trailers`) at its tip `5e98a9f`; rebased onto `main` once T-042 merges |
| Touches | `plugins/git/src/commit_message.rs`, `plugins/git/src/lib.rs` (its doc), its tests and the test repository, `plugins/git/rules/commit-message.md`, `plugins/git/CHANGELOG.md` |

## Goal

Today `git/commit-message` reads only the message being written, the one the `commit-msg` hook
passes, so a commit made without the hook (`git commit -n`, a cherry-pick, a rebase, an edit in a
web interface) is never checked, and the CI job never sees a message. commitlint covers this in CI
with `--from <base> --to <head>`. With this task the rule also checks the message of each commit a
`gate check --push` run pushes, with every aspect, as `git/no-ci-skip` already reads both the
message being written and history.

## Design

- A run with a push range (`gate check --push`, the `pre-push` hook and the CI job that runs it;
  `git.range()` with a `target_ref`) checks, besides the message being written when there is one,
  the message of each commit of `<base>..<head>`, oldest first, with every aspect and option:
  the form checks, `ignores` and the default ignores (so the merges and reverts of the range are
  skipped, as commitlint skips them), and `prohibited-trailers`.
- A finding of a pushed commit is on that commit (`Location::commit`), its target the commit id
  (so an exception names the commit, as for the history rules), its key the commit id, and the
  trailer after it for `prohibited-trailers`; its message starts with the short id
  (`a1b2c3d4e5f6: type feature is not a commit type; ...`). A finding of the message being
  written is as today.
- A new branch (no base): the head commit alone. The rule reads no `[plugins.git] options`. A pull
  request's CI range always has a base, so the gap is only the first `pre-push` of a new branch,
  and the CI job closes it.
- A `--rev` run (a range without a target ref) checks nothing more than today.

## Breaking, for consumers

A `gate check --push` that passed before can fail after the upgrade: a pushed commit whose message
breaks the rule (made with `git commit -n`, cherry-picked, rebased or edited on the web) is now
reported by the `pre-push` hook and by the CI job, under the same aspects as in the `commit-msg`
hook. Such a commit is reworded before the push (`git rebase -i`), or, when already shared, given
an exception by its commit id. Commits already on the remote ref before the push are outside the
range and are not checked. Since it can turn a passing push into a failing one, it ships in v0.3.0
and the changelog lists it under `Breaking`.

## The lead's answers

2026-11-19: approved: the range's commits, oldest first, every aspect, the ignores as commitlint
applies them, findings on the commit with its id as target and key; v0.3.0, under `Breaking`;
`--rev` runs stay as they are. A new branch: not tied to `main`; the lead asked for what
`git/no-ci-skip` does with a zero base, through one shared function. `git/no-ci-skip` has no such
case: it never reads the range's base, and reads the policy history instead (`activation..main`,
main the pushed head when the push targets a main ref, plus `main..HEAD`), which needs
`activation` and `main`. The lead chose: the head commit alone for a new branch, not tied to
`main`, since a pull request's CI range always has a base and closes the gap of a new branch's
first `pre-push`.

## Open item, for both rules

The commits a new branch's head adds over every remote ref, for `git/commit-message` and
`git/no-ci-skip` alike. It needs a git query the plugin interface may not have yet; the lead decides
it separately, outside this task.

## Done when

- Tests: a push range whose commits all pass; one commit failing a form aspect and one failing
  `prohibited-trailers`, each on its commit with its target and key; a merge and a `fixup!` in the
  range skipped by the default ignores; a new branch (the head commit alone); a `--rev` run and a
  run with neither range nor message checking nothing; the message being written and a push range
  in one run.
- The rule page (what is read, when) and the changelog under `0.3.0 (unreleased)`, `Breaking
  Changes`.
- `rustfmt --edition 2024 --check` on the changed `.rs` files exits 0 before each commit.
- fmt, clippy, test (`--no-fail-fast`), deny, build and the plugins' wasm builds exit 0 in one lane
  round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-055-commit-message-push` from `origin/main` at `f0d7ca4`; this file
first, with the design, sent to the lead before any code.
2026-11-19: at the lead's word, coded on the T-042 tip `5e98a9f` before T-042 merges (the three
design commits rebased onto it; to be rebased onto main after T-042 merges). `check` reports the
message being written as before, then each commit of the push range, oldest first, through one
`report`: `pushed` reads `git.log("<base>..<head>")`, or `git.log("<head>^!")` (the head commit
alone) for a new branch, and nothing when the range has no target ref (`--rev`). A pushed commit's
findings are on `Location::commit`, start with its 12-character id, have the commit id as target
and key (the key `<id> <trailer>` for `prohibited-trailers`). The test repository gains
`set_push(base, head, target_ref)` and reads `<commit>^!` in `log`. Tests: the six of "Done when".
Title, module and option docs, rule page (what is read, when, an example finding of a pushed
commit, the difference for a new branch) and the changelog under `# 0.3.0 (unreleased)`,
`### Breaking Changes`, above the v0.2.1 `## Unreleased`, as T-048's branch heads it, with the
consumer paragraph. rustfmt --edition 2024 --check on the changed `.rs` files: exit 0.
2026-11-19: lane ticket 237 on `3dde7ae`: fmt 0, clippy 101 (`clippy::assert_is_empty` on an
`assert!(!keys.is_empty())` in the merge-and-fixup test), so the round stopped after clippy. The
assertion is dropped: the one after it already needs both keys present. rustfmt --edition 2024
--check on the changed `.rs` files: exit 0.
2026-11-19: lane ticket 246 on `4df4233` (log `T-055/lane/run-2.log` outside the repository): fmt
0, clippy 0, test 101 (1022 passed, 2 failed, 7 ignored, summed from its `test result:` lines),
deny 0, build 0, and the eight plugins' wasm builds 0. Both failures were the tests' data: the
passing push's `docs:` commit had no scope, which `scope-empty` requires, and the failing commit
whose body ends in its trailer also fails `body-full-stop`. The passing push now writes
`docs(guide):`, and the failing commit's test expects `body-full-stop` on that commit too, before
`prohibited-trailers`. rustfmt --edition 2024 --check on the changed `.rs` files: exit 0.

# T-045: The documents show the first commit

| Field | Value |
| --- | --- |
| Status | merged |
| Release | v0.2.0 |
| Writer | "Gate 004: public presentation" |
| Reviewer | the lead reads the documents |
| Depends on | `v0.2.0` |
| Touches | `README.md`, `docs/guide/getting-started.md`, `docs/tasks/T-045-commit-message-docs.md` |

## Goal

A newcomer who follows the README or the getting-started guide makes the first commit with a message
the documents' own `git/commit-message` accepts. The newcomer run on the released v0.2.0 (T-032's
Proof) found every documented command passing, but the first commit refused where the documents
wire the `commit-msg` hook: `chore: add gate` names no scope, and the documents showed no message.
The rule's default stays: it follows the house commitlint configuration, where a scope is required.

## Scope

1. Wherever the reader commits, in the README (one project and monorepo) and in getting-started,
   the exact command `git commit -m "chore(gate): add gate"`, then one sentence: the `commit-msg`
   hook checks `<type>(<scope>): <subject>`, a scope is required by default, and the rule page
   (`plugins/git/rules/commit-message.md`) explains how to change that.
2. The README's opening note says `v0.2.0` is the current release, in a sentence or two.
3. The release page installs no hook, so its steps do not change.

## Done when

- The stranger run on the released v0.2.0, with the documents' new text as written, each exit code
  named in this file.
- The leak scan named in the local rules prints nothing.
- The lead has read the documents.

## Proof

Stranger run on `7be53b9` (2026-11-19T17:36Z to 17:39Z): an anonymous clone of this branch, the
release page's body read from the public API, the released v0.2.0 binaries and plugins; every fenced
bash block of `README.md` and `docs/guide/getting-started.md` run line by line as written in a fresh
repository with no token, an empty gh configuration and a fresh cache; the toml blocks written as
given, the activation placeholder replaced by `git rev-parse HEAD` as the text says. The commits are
the documents' own `git commit` lines; the release page's "commit all three" is the one commit the
run writes itself.

| Scenario | Commands | Exit codes |
| --- | --- | --- |
| release page | 13 | all 0 |
| README, one project (the commit-msg hook wired) | 16 | all 0 |
| README, monorepo (pnpm workspace, an `origin`) | 17 | all 0 |
| getting-started (hooks, then CI, each committed) | 22 | all 0 |

After each scenario `bin/gate sync --check` exits 0 and `git status --short` lists nothing. The
monorepo scenario ran twice: the first run's `gate lock` could not reach github.com (curl exit 28
after 75 s, `gate lock` exit 2, and the two commands after it exit 2 on the missing pins); the rerun
at 17:39Z is the one above. An earlier run on `22750cb` refused the guide's second commit,
`chore(gate): add the CI job` (`header-case`, `subject-case`); `7be53b9` makes it lower case.

## Resume point

2026-11-19T17:53:44+00:00: merged into `main` at `2ae4d79` (branch tip `de14256`), after the
`v0.2.0` tag; it changes documents only. Nothing is left.

Earlier:

2026-11-19: written, pushed and proved on the released v0.2.0. Next: the lead's read.

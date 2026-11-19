# T-045: The documents show the first commit

| Field | Value |
| --- | --- |
| Status | in progress |
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

## Resume point

2026-11-19: task file written. Next: the README and getting-started, then the stranger run.

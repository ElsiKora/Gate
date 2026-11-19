# T-032: The newcomer path works as written

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 004: public presentation" |
| Reviewer | a separate review session (the workflow change); the lead reads the documents |
| Depends on | the v0.1.0 root |
| Touches | `README.md`, `docs/guide/**`, `docs/architecture.md`, `plugins/git/rules/main-history.md`, `docs/plan.md`, `docs/tasks/**`, `.github/workflows/release.yml`, `.github/workflows/release-plugin.yml` |

## Goal

A newcomer who pastes the README's and the getting-started guide's commands into a fresh repository
gets each command's documented result. The README describes the released v0.1.0. No document names
the local job queue, its ticket numbers, or a commit of the history before the public root. A core
release opens with its install steps and becomes the repository's latest release; a plugin release
never does.

## Scope

1. The example configurations of the README, the guides, `docs/architecture.md` and
   `plugins/git/rules/main-history.md`: `activation` holds the output of `git rev-parse HEAD`, and
   the text says so instead of offering a placeholder commit.
2. The commands after the example configuration track the files they need (`git add`) before
   `lock`, `sync` and `check`.
3. The install steps run in the repository root and remove the downloaded archive, `SHA256SUMS`
   and `gate` after `gate init`.
4. The README's opening note and Roadmap describe the released v0.1.0; the plugins row names `ci`.
5. The README FAQ's unwrapped line is wrapped at 100 columns.
6. No wording of the local job queue or its ticket numbers in T-025, T-026, T-027 and
   `docs/plan.md`; no commit id of the history before the public root in any document, nor a CI
   run of the deleted repository.
7. The house-style guide says how this repository's changelog is written; the README's Project
   Structure lists `examples/`, `scripts/` and `spike/`.
8. `release.yml`: the core release body starts with an Install section with its version filled in,
   and the release is marked latest. `release-plugin.yml`: plugin releases are created with latest
   false.

Not in scope: `gate init`'s suggested rule and the commit-msg check (T-029).

## Done when

- The stranger run on the pushed branch: every README and getting-started command pasted as written
  into a scratch repository, each exit code named in this file.
- A grep of `docs` for the queue's words and ticket numbers finds none (T-029's branch `:ticket`
  placeholder aside), and no document names a commit of the earlier history.
- The workflow change: `actionlint` and the leak scan exit 0 in the gates on the pushed commit, and
  a review.

## Notes

- Gate reads the working-tree content of a tracked file, so an edit to `gate.toml` needs no new
  `git add` before `lock` or `check`; but the pre-commit hook checks the staged files, so every file
  Gate writes or the reader edits is added again before the commit.
- T-029 changes the README's and getting-started's git rule and hook lines; this task leaves them
  to it and rebases onto main after it merges, then the stranger run is repeated on that tip.

## Proof

Stranger run, interim, on `55b8c5c` (2026-11-19T03:25Z): an anonymous clone of the branch; every
fenced bash block of `README.md` and `docs/guide/getting-started.md`, read from the files, run line
by line in a fresh repository with no token, an empty gh configuration and a fresh cache; the toml
blocks written as given, the activation placeholder replaced by `git rev-parse HEAD` as the text
says. Each line's exit code:

| Scenario | Blocks | Commands | Exit codes |
| --- | --- | --- | --- |
| README, one project | install; the commands after the example; the commit | 16 | all 0 |
| README, monorepo (pnpm workspace, an `origin`) | install; the monorepo commands; the commit | 17 | all 0 |
| getting-started | install; lock; check; explain and list; hooks; CI; the commit | 21 | all 0 |

After each scenario `bin/gate sync --check` exits 0 and `git status --short` lists nothing. The
README's "From Source" and "Development Setup" blocks are the gates of the pushed commit.

## Resume point

2026-11-19: written and pushed; the interim proof passes. Next: rebase onto main after T-029 merges,
repeat the stranger run on that tip, then the gates (actionlint, leak scan) and the review of the
workflow change.

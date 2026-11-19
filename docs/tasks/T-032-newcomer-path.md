# T-032: The newcomer path works as written

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 004: public presentation" |
| Reviewer | a separate review session (the workflow change); the lead reads the documents |
| Depends on | the v0.1.0 root |
| Touches | `README.md`, `docs/guide/**`, `docs/plan.md`, `docs/tasks/**`, `.github/workflows/release.yml`, `.github/workflows/release-plugin.yml` |

## Goal

A newcomer who pastes the README's and the getting-started guide's commands into a fresh repository
gets each command's documented result. The README describes the released v0.1.0. No document names
the lane, a lane ticket, or a commit of the history before the public root. A core release opens
with its install steps and becomes the repository's latest release; a plugin release never does.

## Scope

1. The README's and getting-started's example configuration: `activation` holds the output of
   `git rev-parse HEAD`, and the text says so instead of offering a placeholder commit.
2. The commands after the example configuration track the files they need (`git add`) before
   `lock`, `sync` and `check`.
3. The install steps run in the repository root and remove the downloaded archive, `SHA256SUMS`
   and `gate` after `gate init`.
4. The README's opening note and Roadmap describe the released v0.1.0; the plugins row names `ci`.
5. The README FAQ's unwrapped line is wrapped at 100 columns.
6. No lane or ticket wording in T-025, T-026, T-027 and `docs/plan.md`; no commit id of the history
   before the public root (`2b248bd`, `550aa0f` and the like) in any document.
7. The house-style guide says how this repository's changelog is written; the README's Project
   Structure lists `examples/`, `scripts/` and `spike/`.
8. `release.yml`: the core release body starts with an Install section with its version filled in,
   and the release is marked latest. `release-plugin.yml`: plugin releases are created with latest
   false.

Not in scope: `gate init`'s suggested rule and the commit-msg check (T-029).

## Done when

- The stranger run on the pushed branch: every README and getting-started command pasted as written
  into a scratch repository, each exit code named in this file.
- `git grep -n -w 'lane\|ticket' -- docs` names no lane or queue ticket, and no document names a
  commit of the earlier history.
- The workflow change: `actionlint` and the leak scan exit 0 in the gates on the pushed commit, and
  a review.

## Resume point

2026-11-19: branch `task/T-032-newcomer-path` from `59fb163`; the task file is the first commit.

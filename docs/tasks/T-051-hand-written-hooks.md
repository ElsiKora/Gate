# T-051: Hooks written by hand

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 (documents only; no version moves) |
| Writer | "Gate 004: public presentation" |
| Reviewer | the lead reads the documents |
| Depends on | `main` at `4c55ca6` |
| Touches | `docs/guide/configuration.md` (`[hooks]`), `docs/tasks/T-051-hand-written-hooks.md` |

## Goal

A repository whose git hooks already run through another manager (Husky, for example) calls Gate
from its own hook files and gets exactly what the hooks `gate sync` writes would do. The guide shows
the lines and says why each is there: the `commit-msg` line, the `pre-push` loop over standard
input with `--push` and `--target-ref`, the zero object id of a deleted ref and of a new branch, and
what `--hook` changes.

## Scope

1. In `docs/guide/configuration.md`, beside `[hooks]`, a part on hooks written by hand. Each line
   is taken from the hooks `gate sync` writes (`crates/gate-host/src/sync/hooks.rs`) and each flag
   from `gate check --help`, at `main`.
2. It says that `[hooks]` and a hand-written hook do not mix: `gate sync` with `[hooks]` points
   `core.hooksPath` at `.githooks`, and without `[hooks]` it leaves `core.hooksPath` alone.
3. No code changes.

## Done when

- A synthetic repository, with the released v0.2.0, where hand-written `commit-msg` and
  `pre-push` hooks and the generated ones give the same exit code for each case, each named here:
  a good and a bad commit message, a commit on a prohibited branch, a new branch with a good and a
  bad name, an update, an update carrying a refused commit, and a deleted ref.
- The leak scan named in the local rules prints nothing.
- The lead has read the documents.

## Proof

To be written.

## Resume point

2026-11-19T17:57:13+00:00: task file written from `main` at `4c55ca6`. `git diff v0.2.0 4c55ca6 --
crates plugins` is empty, so the released v0.2.0 binary is `main`'s code. Next: the guide's text,
then the proof.

# T-051: Hooks written by hand

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.2.1 (documents only; no version moves) |
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

Run on 2026-11-19T19:02Z with the released v0.2.0 (`gate 0.2.0`, its archive checked against
`SHA256SUMS`). `git diff v0.2.0 4c55ca6 -- crates plugins` is empty, so its hooks and its `gate
check --help` are `main`'s: `--push <RANGE>` "Check a pushed range <base>..<tip> (a base of zeros is
a new branch)", `--target-ref <REF>` "The remote ref the --push range goes to", `--commit-message
<FILE>` "the commit-msg hook passes "$1".

Two synthetic repositories, each with a local bare `origin`, the `git` plugin at 0.2.0 and the rules
`git/commit-message`, `git/no-ci-skip` and `git/branch-name` (template `:type/:ticket-:name`), no
global git configuration and no token. One has `[hooks]` with `commit-msg` and `pre-push` and runs
`gate sync`; the other has no `[hooks]`, the two hooks taken by script from the two `sh` blocks of
this branch's guide at `922aed5`, in `.husky/`, and `core.hooksPath` set to `.husky` as Husky sets
its own directory (Husky itself is not installed). The hand-written files equal the generated ones
without their second, comment line. The same cases ran in each:

| Case | Command | Generated | Hand-written |
| --- | --- | --- | --- |
| a commit on `main` (prohibited in a hook run) | `git commit --allow-empty -m 'feat(app): add a note'` | 1 | 1 |
| a plain check on `main` (no `--hook`) | `bin/gate check` | 0 | 0 |
| a bad message | `git commit -m 'added login'` | 1 | 1 |
| a good message | `git commit -m 'feat(app): add login'` | 0 | 0 |
| a new branch, good name (remote id of zeros) | `git push origin feature/ab-1-login` | 0 | 0 |
| a new branch, bad name (`--target-ref`) | `git push origin HEAD:wip-login` | 1 | 1 |
| an update | `git push origin feature/ab-1-login` | 0 | 0 |
| an update carrying `[skip ci]` | `git push origin feature/ab-1-login` | 1 | 1 |
| a deleted ref (local id of zeros) | `git push origin --delete bugfix/ab-2-old` | 0 | 0 |
| a deletion and a new branch in one push | `git push origin :refs/heads/feature/ab-1-login HEAD:refs/heads/bugfix/ab-3-new` | 0 | 0 |

Every case's output, with object ids and the origin's directory name replaced, is the same in both
repositories (10 of 10); the deletion alone prints nothing, so no check ran for it. The refusals
name `git/branch-name` (`branch main is prohibited`; `wip-login does not match the template`),
`git/commit-message` (`scope-empty`, `subject-empty`, `type-empty`) and `git/no-ci-skip`. `gate
doctor` says `core.hooksPath is .githooks` in the first and `gate.toml has no [hooks]` in the
second. The script and its log are kept outside the repository.

## Resume point

2026-11-19T19:03:13+00:00: the guide's part is pushed and proved on the released v0.2.0. Next: the
lead's read.

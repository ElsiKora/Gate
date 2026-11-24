# T-103: Gate checks itself with the released 0.3.2

| Field | Value |
| --- | --- |
| Status | in review |
| Release | after `v0.3.2` (configuration and documents only; no version moves) |
| Writer | a worker session of the lead |
| Reviewer | the lead reads the change |
| Depends on | `main` at `d8f213a` (T-102 merged; `v0.3.2` and `plugin-nestjs-v0.3.2` released) |
| Touches | `gate.toml`, `gate.lock`, `.github/workflows/gate.yml` (generated), `docs/plan.md`, this task file |

## Goal

Gate's own `gate.toml` checked the repository with the released 0.3.0 core. With `v0.3.2` published,
it should use the core it ships. The plugins it uses, `git`, `agents`, `markdown` and `ci`, have no
release after 0.3.0, so their pins stay.

## Scope

1. `gate.toml`: `[gate] version = "0.3.2"`; the four plugin pins stay at `@0.3.0`.
2. `bin/gate lock`, so that `gate.lock` takes the released core's URL and sha256 for each platform.
3. `bin/gate sync`, so that the generated workflow and the hooks are 0.3.2's output. `[ci.github]`
   stays as it is: no `cache`, no `aggregate-name`; turning them on is a separate decision.
4. Whatever 0.3.2 reports on the repository that 0.3.0 did not is listed below and goes to the lead;
   no rule changes to hide it.
5. `docs/plan.md`: a row for this task.

## The path to main

T-094's upgrade branch is for a repository that pins a plugin it builds itself, under a core release
that changes the plugin interface. Neither holds here: `v0.3.2` keeps the plugin interface at
`gate:plugin@0.3.0` (T-101), so the 0.3.0 plugins load under it, and Gate pins only released assets
of its own repository. The root `gate.toml` enables no `git/main-history` and no `git/no-ci-skip`
(`bin/gate check` runs 7 rules: `git/commit-message`, `git/branch-name`, two `agents`, one `markdown`,
two `ci`), so no release form judges the pin move. It goes to `main` the way every task does: this
branch, `task/T-103-self-check-0.3.2`, which the root `git/branch-name` pattern admits as
`task/T-[0-9]{3}[a-z]?-[a-z0-9.-]+`, merged by the lead. The pre-push hook runs
`bin/gate check --push` with the core the pushed `gate.lock` pins.

## Proof

- The releases, read anonymously from the GitHub API (`/repos/ElsiKora/Gate/releases`) on
  2026-11-24: the newest tags of the four plugins are `plugin-git-v0.3.0`, `plugin-agents-v0.3.0`,
  `plugin-markdown-v0.3.0` and `plugin-ci-v0.3.0`; the releases after `v0.3.0` are `v0.3.1`, `v0.3.2`,
  the five plugins at 0.3.1 and `plugin-nestjs-v0.3.2`. `plugins/{git,agents,markdown,ci}/Cargo.toml`
  name `0.3.0` at `d8f213a`.
- Before the move, with 0.3.0: `bin/gate check` exit 0, `gate check (tree): 7 rules, 0 errors, 0
  warnings; projects: ran .; skipped none`.
- `bin/gate lock` (with 0.3.0, anonymous): exit 0, `resolved core 0.3.2`, the four plugins `kept`.
  The new sha256 are those GitHub records for the `v0.3.2` assets: `09956c01...` for
  `gate-aarch64-apple-darwin.tar.gz`, `0adf6bf6...` for `gate-x86_64-unknown-linux-musl.tar.gz`.
- `bin/gate --version`: `gate 0.3.2`. `bin/gate sync`: exit 0, `4 generated, 1 written, 0 removed`.
  The one file written is `.github/workflows/gate.yml`, and its one changed line is
  `cancel-in-progress`, which now cancels a superseded run on every ref but the default branch
  (T-099). The three hooks are byte for byte those 0.3.0 wrote.
- After the move, with 0.3.2: `bin/gate check` exit 0, `gate check (tree): 7 rules, 0 errors, 0
  warnings; projects: ran .; skipped none`. Nothing new to report.
- `bin/gate doctor` with 0.3.2: every line `ok` (launcher 0.3.2, the lock, the cache, the four plugins
  present, verified and compiled under 0.3.2) but `hooks`, which names a `core.hooksPath` set in this
  machine's worktree configuration, outside the repository.
- On `3393b86`: `bin/gate check --push origin/main..HEAD` exit 0, `bin/gate check` exit 0, both
  `7 rules, 0 errors, 0 warnings`; the pre-push hook of the first push, `check --push` with 0.3.2,
  exit 0.

## The workflow "Gate" and the branch

The generated workflow runs on a push to `main` and on a pull request into it (`[ci.github] branches
= ["main"]`), so a pushed task branch gets no "Gate" run; only the "Reproducible" workflow runs on it.
The workflow's three `bin/gate` steps were replayed instead, as a push to `main` would run them after
the merge: in an `x86_64` Linux container (`alpine:3.20`, `--platform linux/amd64`), on a fresh
anonymous clone with `main` merged with this branch by `git merge --no-ff`, an empty
`GATE_CACHE_DIR` and no token. `bin/gate` fetched the `x86_64-unknown-linux-musl` core of `v0.3.2`
and the four plugins and verified them; `gate 0.3.2`. With `GATE_RANGE` `d8f213a..<merge>` and
`GATE_TARGET_REF` `refs/heads/main`: `bin/gate sync --check` exit 0 (`4 generated files, 0 out of
date`), `bin/gate check --list-changed-projects ...` exit 0 (`projects=["."]`), `bin/gate check --push
... --no-overrides` exit 0 (`7 rules, 0 errors, 0 warnings`). The same three steps with the range
`d8f213a..3393b86` and the branch as target ref exit 0 each, with the same lines. The first "Gate"
run under 0.3.2 is the push of the merge to `main`.

## Done when

- `bin/gate check` and `bin/gate check --push <range>` on this branch exit 0 with 0.3.2.
- The GitHub workflow "Gate" passes on the pushed branch.
- The leak scan named in the local rules prints nothing before each push.
- `scripts/release-check.sh` is not run: no version moves.

## Resume point

2026-11-24T18:54:05+00:00: on `task/T-103-self-check-0.3.2` off `main` at `d8f213a`; the move is
`3393b86`, pushed, checked locally and in the Linux replay above. Next: the lead merges; the push of
the merge to `main` is the first "Gate" run under 0.3.2.

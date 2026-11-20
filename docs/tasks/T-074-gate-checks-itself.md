# T-074: Gate checks itself

| Field | Value |
| --- | --- |
| Status | done; ready for the lead |
| Release | 0.3.0 timeframe, built on the released 0.2.1 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `cee6491` |
| Touches | `gate.toml`, `gate.lock`, `bin/gate`, `.githooks/*`, `.github/workflows/gate.yml` (all new, at the root), `AGENTS.md` (the commit types and one line on the hooks); nothing under `crates/` or `plugins/` |

## Goal

Gate's own repository is checked by Gate. It gets a root `gate.toml`, a `gate.lock` and `bin/gate`, all
pinned to the released core v0.2.1 and to released plugins. `git/commit-message` refuses AI
attribution trailers, as required by the "No AI attribution" rule in [AGENTS.md](../../AGENTS.md).
`git/branch-name` holds branches to the task, lead and release forms.

## Design

### Pins

| Pin | Version | Why |
| --- | --- | --- |
| core | v0.2.1 | the released core; `gate lock` pins `aarch64-apple-darwin` and `x86_64-unknown-linux-musl` |
| git | 0.2.1 | commit message and branch name |
| agents | 0.2.0 | `AGENTS.md` references and no `CLAUDE.md` |
| markdown | 0.2.0 | local links in the documents |
| ci | 0.2.0 | the workflows: actions pinned to a commit, no `workflow_run` on untrusted code |

Every checksum in `gate.lock` equals the sha256 digest that GitHub lists for the released asset. The
same digests are in each release's `SHA256SUMS` or `<plugin>.wasm.sha256`.

Three other plugins were left out:

- `repository`, `docker` and `node` are not released at a version that fits this repository.
- `rust` needs options that only a project can give.

### Configuration

```toml
[gate]
version = "0.2.1"

[plugins.git]
source = "ElsiKora/Gate:git@0.2.1"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]

[plugins]
agents = { source = "ElsiKora/Gate:agents@0.2.0" }
markdown = { source = "ElsiKora/Gate:markdown@0.2.0" }
ci = { source = "ElsiKora/Gate:ci@0.2.0" }

[rules]
"git/commit-message" = { level = "error", levels = { header-min-length = "warning", scope-max-length = "warning", header-case = "off", subject-case = "off", body-full-stop = "off" }, options = { types = ["feat", "fix", "test", "docs", "ci", "chore", "refactor", "perf", "style", "revert"], subject-max-length = 0, ignores = ['^Merge (task|lead)/'], prohibited-trailers = [
  '^[Cc]o-[Aa]uthored-[Bb]y: .*([Cc]laude|[Aa]nthropic|[Cc]opilot|[Oo]pen[Aa][Ii]|[Cc]hat[Gg][Pp][Tt]|[Gg]emini|[Cc]odex|[Cc]ursor|\b[Aa]gent\b|\b[Bb]ot\b|\bAI\b|\b[Aa]ssistant\b)',
  '^[Gg]enerated-([Ww]ith|[Bb]y): ',
] } }
"git/branch-name" = { level = "error", options = { pattern = '^(main|task/T-[0-9]{3}[a-z]?-[a-z0-9.-]+|lead/[a-z0-9.-]+|release/v[0-9]+[.][0-9]+[.][0-9]+|(agent|claude)/[A-Za-z0-9._/-]+)$', min-length = 4, max-length = 80, prohibited = ["master"] } }
"agents/instruction-references" = "error"
"agents/no-claude-md" = "error"
"markdown/local-links" = { level = "error", options = { roots = ["docs", "plugins"], extra-files = ["README.md", "AGENTS.md", "CHANGELOG.md"] } }
"ci/actions-pinned" = "error"
"ci/workflow-run-target" = "error"

[hooks]
pre-commit = ["check --staged"]
commit-msg = ["check --commit-message"]
pre-push = ["check --push"]

[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"   # v7.0.1, as in the release workflows
branches = ["main"]
aggregate = "gate-passed"

[ci.github.jobs.gate]
name = "Gate"
run = ["check --push"]
```

The choices in `git/commit-message`:

- **Merges.** `ignores` exempts `Merge task/...` and `Merge lead/...`, the form main uses for every
  merge. Without it, each merge gives a `type-empty`, a `subject-empty` and a `scope-empty`.
- **Header and subject case** are off. Subjects here begin with a lower-case word, and the case
  aspects misread names such as `SHA256SUMS`, `README` and `T-064`.
- **`subject-max-length` is 0**, which turns it off. The header length limit of 100 already bounds
  the subject.
- **`body-full-stop` is off.** In git 0.2.1 the last paragraph counts as body even when it is a
  trailer paragraph. Any message with a trailer, including a person's `Co-authored-by`, then fails
  `body-full-stop`. This was measured; see "Proof on an export" below. The fix belongs in a git
  plugin follow-up.
- **Types.** AGENTS.md lists feat, fix, test, docs, ci and chore. The history on main also uses
  perf, refactor and style. Decision 1 below asks which list to keep.

### Proof on an export

Every check ran on `git clone --no-hardlinks` of this worktree at `cee6491`, with the three new files
staged. None ran on the live repository, and the export's `core.hooksPath` is its own.

- **Tree.** `bin/gate check` printed `7 rules, 0 errors, 0 warnings`. The existing release
  workflows already pass `ci/actions-pinned` and `ci/workflow-run-target`.
- **Generated files.** `bin/gate sync` wrote `.githooks/pre-commit`, `.githooks/commit-msg`,
  `.githooks/pre-push` and `.github/workflows/gate.yml`. `bin/gate sync --check` then printed
  `4 generated files, 0 out of date`.
- **Trailers.** These results come from `bin/gate check --commit-message <file>` and from real
  `git commit` runs through the hooks:
  - `Co-Authored-By: Claude <...>` is refused by `prohibited-trailers`. The hook stopped the commit.
  - `Generated-with: an assistant` is refused by `prohibited-trailers`.
  - `Co-authored-by: A Person <...>` passes once `body-full-stop` is off. With `body-full-stop` on,
    it fails that aspect even though the body ends with a full stop.
  - A body line `Generated with an assistant` is **not** refused. It is not a trailer, and no git
    0.2.1 aspect reads body lines for a pattern. Decision 4 below covers this.
- **History.** Each of the 346 commits on `origin/main` (41 on the first-parent line) was given to
  `bin/gate check --commit-message --format json`:
  - With the reference defaults and only AGENTS.md's six types, 243 commits failed, with 594
    findings. The largest groups were header-case (191), subject-case (153), and the merges
    (scope-empty 44, type-empty 37, subject-empty 37).
  - With the configuration above, 59 commits fail, with 66 errors and 4 warnings:

    | Aspect | Findings | Example |
    | --- | --- | --- |
    | body-max-line-length | 31 | `f2d6a48`: body line 1 is 321 characters long |
    | header-max-length | 14 | headers of 103 to 189 characters, e.g. `9ece780` |
    | footer-leading-blank | 13 | `ea1296f`: a `[#n]` reference in the body reads as a footer |
    | scope-empty | 7 | `329bdf2`: `docs: the examples name the 0.2.1 release` |
    | scope-case | 1 | `ce3e118`: scope `T-064` |
    | scope-max-length (warning) | 4 | |

  - No commit has a prohibited trailer.

  So the history on main does **not** pass. That does not block this task, because
  `git/commit-message` checks only the message being written, in the `commit-msg` hook. Neither
  `check --push` nor CI reads old messages. The full list is in the lead's notes folder as
  `T-074-history-findings.txt`.
- **Branches.** 112 names were tested: the remote and local branches, plus `lead/batch-2`.
  - With the pattern from the brief, 40 names failed. The failures included the dotted task names,
    such as `task/T-070-v0.2.1-version`, and `release/v0.2.0`. The pattern above therefore allows
    dots and `release/vX.Y.Z`.
  - With the widened pattern, the only failing task, lead and release names are three old ones:
    `task/T-014`, `task/T-015` and `task/nestjs-no-converter`.
  - The other failures are local working names: `backup/*`, `scratch/*`, `fix/*`, `rebase/*`,
    `g002/*`, `m2`, `m3`, `T-007b-*`, and the desktop app's worktree branches `agent/*`.
    The list is in the notes folder as `T-074-branch-findings.txt`.
- **`main` in hook runs.** With the default `prohibited` list (`main`, `master`, `release`), both
  `check --commit-message --hook` and `check --push ... --target-ref refs/heads/main --hook` on
  `main` gave `branch main is prohibited`. That would block the lead's own merges and pushes to
  main. Without `--hook`, as in CI, `main` passes. The configuration therefore sets
  `prohibited = ["master"]`.

### Hooks

`gate init` writes `bin/gate` and `gate.lock` and installs no hook. `gate sync` writes the hooks
listed in `[hooks]` to `.githooks/` and sets `core.hooksPath=.githooks`:

| Hook | Runs | Checks |
| --- | --- | --- |
| `pre-commit` | `bin/gate check --staged --hook` | the staged files |
| `commit-msg` | `bin/gate check --commit-message "$1" --hook` | the message, including the trailers; branch name |
| `pre-push` | `bin/gate check --push <remote>..<local> --target-ref <ref> --hook`, once per pushed ref | the pushed range; branch name of the target |

`core.hooksPath` lives in the shared `.git/config`, so the hooks run in every worktree.
One `bin/gate sync` after the merge turns them on for all worktrees of that clone.

### CI

A generated `.github/workflows/gate.yml` goes next to `release.yml`, `release-plugin.yml` and
`reproducible.yml`. `gate sync` leaves those three alone, because they lack the "Generated by gate
sync" marker. The generated workflow:

- runs on pushes and pull requests to `main`, on `ubuntu-24.04`, with `contents: read`;
- checks out with `persist-credentials: false`;
- has three jobs:
  - `changes` runs `sync --check` and lists the changed projects;
  - `gate` runs `check --push <range> --target-ref <ref> --no-overrides`;
  - `gate-passed` is the aggregate job that branch protection can require;
- uses the core's `x86_64-unknown-linux-musl` pin from `gate.lock`;
- reads `GATE_GITHUB_TOKEN` from the secret, with `github.token` as the fallback.

In CI, `branch-name` runs without `--hook`, so a push to `main` passes. A commit made with
`--no-verify` skips the `commit-msg` hook, and CI does not catch it, because 0.2.1 reads no message
in a pushed range. Decision 5 below covers this.

### Hooks on a shared `core.hooksPath`

`core.hooksPath` is shared by every worktree of a clone, and this machine has 113 worktrees of this
repository (`git worktree list`). Most are on branches that have no `gate.toml` and no `bin/gate`.
This was proved on a scratch clone with `extensions.worktreeConfig` on, as on this machine. After
`bin/gate sync` set `core.hooksPath=.githooks`:

- **A worktree on a branch from `cee6491`, without the files, is neither blocked nor broken.** The
  relative path resolves in each worktree, and that worktree has no `.githooks/`, so git runs no hook.
  `GIT_TRACE=1 git commit` shows no hook, and the commit took 0.08 s.
- **The cost stays small.** At a load average of 29 (`uptime`), three commits in a row through
  the hooks took 1.14 s, 0.42 s and 0.42 s, the first with a cold cache. Ten commits started at once in
  ten worktrees each took between 1.03 s and 1.29 s, and all exited 0.
- **The hooks work in practice.** These all ran through the hooks with the final configuration:
  - a `Merge task/...: ...` merge on `main` passed;
  - a commit on a `claude/*` branch passed;
  - a message without a type was refused.
- **The lane's round worktrees run no hook.**
  - The lane script runs no git command.
  - A round script runs only `git worktree add --detach`, `git log` and `git worktree remove`, and
    `GIT_TRACE=1` shows no hook for them. Gate generates no `post-checkout` hook.
  - As a guard, a round script turns hooks off in its own worktree with
    `git -C <round-wt> config --worktree core.hooksPath /dev/null`. On the scratch clone, a worktree
    with that setting committed a message without a type, and its neighbour without the setting
    refused the same message.
  - T-066b's round script now does this. It lives outside the repository.

`core.hooksPath` is not set on the live repository by this task. After the merge, one `bin/gate sync`
on `main` turns the hooks on for every worktree of that clone, as the line added to AGENTS.md says.

## Decisions (the lead's answers)

1. **Types: (a).** AGENTS.md now lists `refactor`, `perf`, `style` and `revert` beside the six.
2. **`prohibited = ["master"]`.** The lead merges on `main` by design. `main` is also in the pattern,
   and `min-length = 4` lets it through: the default `min-length` of 5 refused it in hook runs.
3. **App worktree branches are allowed.** The pattern also takes `agent/*` and `claude/*`, and
   `max-length = 80` fits the app's names. In hook runs with `--commit-message` and with `--push`,
   these names pass on the scratch clone: `main`, `agent/t-011-public-presentation-1bd727`,
   `claude/t-068-frontend-plugin-7143a8`, `lead/batch-3`, `task/T-074-gate-checks-itself` and
   `release/v0.3.0`. These are refused: `master`, `feature/x` and `mainline`.
4. **"Generated with" body lines** are a follow-up, T-075. See "Follow-ups".
5. **`--no-verify` in CI** is a follow-up: move the pin to 0.3.0 at release. See "Follow-ups".
6. **`agents/rule-references` and `markdown/frontmatter` stay off.**

## Follow-ups

- **T-075 (git plugin 0.3.0, assigned by the lead).**
  - Add a prohibited body-line pattern to `git/commit-message`, so that a `Generated with ...` body
    line is refused. 0.2.1 lets it through, measured with `bin/gate check --commit-message`.
  - Make `body-full-stop` ignore the trailer paragraph. In 0.2.1, a person's `Co-authored-by` fails
    it, so `body-full-stop` stays off here until then.
- **Pin 0.3.0 at release.** On 0.3.0, `git/commit-message` checks every commit a push adds (T-055).
  Moving this repository's pins to 0.3.0 at release therefore catches a `--no-verify` commit in CI.
  `main-history` and `no-ci-skip` stay off for now.

## Done when

- [x] `gate.toml`, `gate.lock`, `bin/gate`, `.githooks/*` and `.github/workflows/gate.yml` are
  committed at the root, and AGENTS.md lists the ten types.
- [x] On the tip:
  - `bin/gate check` gives `7 rules, 0 errors, 0 warnings`;
  - `bin/gate sync --check` gives `4 generated files, 0 out of date`;
  - `bin/gate lock` leaves `gate.lock` unchanged.
- [x] Through `bin/gate check --commit-message`:
  - an AI `Co-Authored-By` trailer and a `Generated-with:` trailer are refused by
    `prohibited-trailers`;
  - a person's `Co-authored-by`, a `Merge task/...` merge and a `refactor(...)` commit pass.
- [x] The hooks on a shared `core.hooksPath` block nothing on branches without the files, as shown
  in "Hooks on a shared `core.hooksPath`" above.
- [x] The leak scan prints nothing. There is no change under `crates/` or `plugins/`, so there is no
  lane round.

## Resume point

2026-11-20T17:06:52+00:00: the design was approved with the lead's six answers. The configuration,
the hooks, the workflow and the AGENTS.md lines were committed and proved: the shared hooks were
checked on a scratch clone, and the branch names in hook runs. Next: the lead's review and merge, then
one `bin/gate sync` on `main`.

2026-11-20T17:00:18+00:00: branch `task/T-074-gate-checks-itself` from `main` `cee6491`. The design
above was measured on an export. Next: the lead's answers to the six decisions, then the files.

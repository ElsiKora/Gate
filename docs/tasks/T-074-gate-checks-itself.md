# T-074: Gate checks itself

| Field | Value |
| --- | --- |
| Status | design; waiting for the lead |
| Release | 0.3.0 timeframe, built on the released 0.2.1 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `cee6491` |
| Touches | `gate.toml`, `gate.lock`, `bin/gate`, `.githooks/*`, `.github/workflows/gate.yml` (all new, at the root), `AGENTS.md` if the lead extends the type list; nothing under `crates/` or `plugins/` |

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
"git/branch-name" = { level = "error", options = { pattern = '^(task/T-[0-9]{3}[a-z]?-[a-z0-9.-]+|lead/[a-z0-9.-]+|release/v[0-9]+[.][0-9]+[.][0-9]+)$', max-length = 60, prohibited = ["master"] } }
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

## Decisions for the lead

1. **Types.** Option (a): extend AGENTS.md to list `refactor`, `perf`, `style` and `revert`, which
   history uses, and keep them in `types`. Option (b): keep AGENTS.md's six and drop the four from
   `types`. Proposal: (a).
2. **`prohibited`.** Proposal: `["master"]`, so that the lead can commit and push on `main`. The
   other option is `[]`.
3. **Desktop worktree branches.** `agent/*` branches fail `branch-name` in every hook, because
   `core.hooksPath` is shared. Option (a): add `agent/[a-z0-9.-]+` to the pattern. Option (b): writers
   work only on `task/*` branches, as AGENTS.md says, and the lead's sessions use `lead/*`.
   Proposal: (b), and the lead's session renames its branch before its next commit.
4. **"Generated with" in the body.** git 0.2.1 cannot refuse it. Proposal: a git plugin follow-up
   adds an aspect for body lines that match a pattern (`prohibited-lines`), and also makes the
   trailer paragraph stop counting as body for `body-full-stop`. Both are changes under `plugins/`,
   outside this task.
5. **History and CI.** `main-history` and `no-ci-skip` stay off. They need an activation commit
   and custom forms, because main merges `task/*` branches and has no `chore(release)` commits.
   Proposal: a follow-up, with commit messages in a pushed range checked by `check --push` in the
   same change, so that CI catches `--no-verify`.
6. **Left off.**
   - `agents/rule-references` would fire on the plugin pages that name rule ids as examples.
   - `markdown/frontmatter` would fire because no page here has front matter.

## Done when

- `gate.toml`, `gate.lock`, `bin/gate`, `.githooks/*` and `.github/workflows/gate.yml` are committed
  at the root, with the lead's answers applied.
- On the pushed tip, `bin/gate check` and `bin/gate sync --check` exit 0.
- These commit messages are run through `bin/gate check --commit-message`. An AI `Co-Authored-By` and
  a `Generated-with:` trailer exit 1; a person's `Co-authored-by` exits 0.
- The leak scan prints nothing. There is no change under `crates/` or `plugins/`, so no lane round is
  needed.

## Resume point

2026-11-20T17:00:18+00:00: branch `task/T-074-gate-checks-itself` from `main` `cee6491`. The design
above was measured on an export. Next: the lead's answers to the six decisions, then the files.

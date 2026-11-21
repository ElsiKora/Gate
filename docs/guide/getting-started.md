# Getting Started

This guide installs Gate in a repository, runs the first check, wires the git hooks and adds the CI job.
It takes a repository with one project; a monorepo adds a few lines, covered in [monorepo.md](monorepo.md).

## 📋 Before You Start

- macOS on Apple silicon or Linux on x86-64
- `git`, `curl` and a POSIX shell
- A git repository; Gate reads only tracked files, so commit or `git add` what you want checked

## 1. Install Gate in the Repository

Gate is installed into a repository, not onto a machine: every developer, hook and CI job then runs the
same pinned version through `bin/gate`. Download the release once to run `gate init`, in the root of
your repository; the last line removes the downloaded files:

```bash
version=0.2.1
target=aarch64-apple-darwin          # or x86_64-unknown-linux-musl
curl -fsSLO "https://github.com/ElsiKora/Gate/releases/download/v${version}/gate-${target}.tar.gz"
curl -fsSLO "https://github.com/ElsiKora/Gate/releases/download/v${version}/SHA256SUMS"
shasum -a 256 -c SHA256SUMS --ignore-missing
tar -xzf "gate-${target}.tar.gz"
./gate init
rm gate "gate-${target}.tar.gz" SHA256SUMS
```

`gate init` writes three files:

| File | What it is | Commit it |
| --- | --- | --- |
| `gate.toml` | the configuration: plugins, rules, hooks, CI | yes |
| `gate.lock` | the resolved URL and sha256 of the core for each platform and of every plugin | yes |
| `bin/gate` | a short POSIX launcher that reads `gate.lock`, fetches and verifies the core once, and runs it | yes |

The core and the plugins are cached outside the repository (`$GATE_CACHE_DIR`, by default your user
cache directory), shared by every worktree, and need no network once cached.

## 2. Add Plugins and Rules

A fresh `gate.toml` names the core's version and nothing else. Add the plugins you want and turn rules
on:

```toml
[gate]
version = "0.2.1"

[plugins]
markdown = { source = "ElsiKora/Gate:markdown@0.2.0" }

[plugins.git]
source = "ElsiKora/Gate:git@0.2.1"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "<the output of git rev-parse HEAD>"  # the commit the policy starts after
release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$'

[rules]
"git/no-ci-skip" = "error"
"git/commit-message" = "error"
```

Write the commit the git rules start after in place of `<the output of git rev-parse HEAD>`: for a
new policy, the output of `git rev-parse HEAD`. A commit before it is never checked.

A plugin entry is a table whose `source` is `<owner>/<repo>:<plugin>@<version>`: the asset
`<plugin>.wasm` of the release `plugin-<plugin>-v<version>`. Turn on the `markdown` rules you want in
the same `[rules]` table, for example:

```toml
"markdown/local-links" = { level = "error", options = { roots = ["docs"], extra-files = ["README.md"] } }
```

In a repository that is a single project (no `[workspace]`), the root `[rules]` holds both
repository-scope rules and the rules that look at the project's own files (project scope), such as
the `markdown` rules. In a monorepo, project-scope rules go in a project's `gate.toml` or in a
`[[profiles]]` entry of the root file; see [monorepo.md](monorepo.md#-profiles).

Every plugin you add or change is resolved and pinned by `gate lock`. Gate reads only tracked files,
so add the three files `gate init` wrote first; add `gate.lock` again after `gate lock` changes it,
because the pre-commit hook checks the staged files:

```bash
git add gate.toml gate.lock bin/gate
bin/gate lock
git add gate.lock
```

A run whose `gate.lock` does not match `gate.toml` fails and tells you to run `gate lock`. See
[configuration.md](configuration.md) for every key, and [plugins.md](plugins.md) for the shared plugins.

## 3. Run the First Check

```bash
bin/gate check
```

Gate checks the tracked files of the working tree. It prints each finding with its rule, location and
message, the norm it enforces if you mapped one, and a one-line summary with the number of errors and
warnings. The exit code is `0` when no finding is an error, `1` when one is, and `2` when the
configuration or the environment is wrong.

To see what the configuration says about a rule (its level, norm and options, the file each came from,
and its exceptions), and every rule that is on or off:

```bash
bin/gate explain markdown/local-links
bin/gate list                      # every rule that is configured, and what is off
```

What a rule checks and which options it takes is on its page, `plugins/<plugin>/rules/<rule>.md` in
this repository; [plugins.md](plugins.md) links them.

## 4. Fix or Except a Finding

Fix the code, or, when a finding is right but must stay for now, add an exception with a reason to
`gate.exceptions.toml` beside `gate.toml`, and commit it:

```toml
# gate.exceptions.toml
[[exception]]
rule = "markdown/local-links"
target = "docs/legacy/index.md:old-site/setup.md"
reason = "the old site's links, removed when the site moves"
```

The target is the one the finding prints, here `<page>:<link destination>`. An exception that matches
nothing is itself reported, so the list only shrinks. See [configuration.md](configuration.md#exceptions).

On a repository with many findings, adopt a rule first with a baseline: `bin/gate baseline write`
stores today's findings in `gate/baseline.json`, which you commit. They are no longer reported; new ones
are, and an entry that no longer matches is reported too, so the baseline only shrinks. As you fix
findings, `bin/gate baseline prune` removes their entries and never adds one.

## 5. Wire the Hooks

```toml
[hooks]
pre-commit = ["check --staged"]
commit-msg = ["check --commit-message"]
pre-push = ["check --push"]
budget = { warn-after = 3, fail-after = 10 }
```

```bash
bin/gate sync
git add gate.toml .githooks
git commit -m "chore(gate): add gate"
```

`gate sync` writes the hooks into `.githooks/` and sets `core.hooksPath` once for the clone; every
worktree of the clone shares it, and the commit adds `.githooks/`. The hooks call `bin/gate`, so
every developer runs the pinned version; they never skip themselves on an environment variable. A
hook may also run one of your steps, for example `run format` to check the staged files. `budget`
makes a hook that takes longer than 3 seconds warn, and one that takes longer than 10 seconds fail.
On a fresh clone, `bin/gate doctor` tells you when `gate sync` has not run yet. Run `gate sync`
after a clone, a core upgrade or a plugin bump: it compiles every plugin, so the first hook does not
wait for it (compiling is not counted toward the budget either way).

The `commit-msg` hook checks only the message being written, before the commit exists: with the
rules above, `git/commit-message` checks its form and `git/no-ci-skip` refuses a CI-skip directive
in it. The tree is checked by `pre-commit`, `pre-push` and CI, not again here. Its summary says so:
`gate check (commit message): ...`. The form is
`<type>(<scope>): <subject>`, a scope is required by default, and the
[rule page](../../plugins/git/rules/commit-message.md) explains how to change that.

## 6. Add the CI Job

```toml
[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1"

[ci.github.jobs.gate]
name = "Gate"
runs-on = "ubuntu-24.04"
timeout-minutes = 15
run = ["check --push --format github"]
```

```bash
bin/gate sync
git add gate.toml .github/workflows/gate.yml
git commit -m "chore(gate): add the workflow"
```

`gate sync` writes the workflow file, and the commit adds it. `checkout` pins the checkout action by
its full commit id, with the release after ` # `, which the workflow keeps as a comment.
`timeout-minutes` is the job's time limit; every job needs one. The workflow's generated first job runs `gate sync --check`, which fails when
the hooks or the workflow drift from `gate.toml`, and lists the projects the change touches. In a
job, `--push` takes no value: the job passes the pull request's or the push's range. Generated
`check` commands run with `--no-overrides`, so no plugin can be swapped in CI, and, when the job
sets `budget = { warn-after = <seconds>, fail-after = <seconds> }`, with those thresholds: a run
over `warn-after` warns and one over `fail-after` fails. With
`--format github` findings show as annotations on the pull request; with `--format sarif` you can
upload them to code scanning.

## Next Steps

- [configuration.md](configuration.md): levels, norms, options files, exceptions, steps and suites
- [monorepo.md](monorepo.md): several projects, each with its own rules
- [writing-a-plugin.md](writing-a-plugin.md) and [writing-a-rule.md](writing-a-rule.md): your own rules

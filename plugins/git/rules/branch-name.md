# `git/branch-name`

The branch a push updates, or else the checked-out branch, has a name the policy allows.

## What it checks and why

A branch name is how a change is found again: in a merge subject, a pull request, a CI run. A name
outside the convention is caught while the branch is still cheap to rename, not after it has been
merged into main's history. Which name the rule checks depends on the run:

- **A run under `--push`** (the pre-push hook): the branch each pushed ref updates. `git push origin
  HEAD:bad` checks `bad`, whatever branch is checked out. A pushed ref outside `refs/heads/` (a tag)
  updates no branch, and is not checked.
- **Any other run** (`gate check`, the pre-commit hook, `--rev`): the checked-out branch, as the host
  reports it.
- **A detached `HEAD`** (a CI checkout of a commit or of a pull request's merge): no branch is
  checked out, and the rule checks nothing.

The name is reported when:

- it does not match `pattern`. The pattern is searched anywhere in the name: `feat/` matches
  `wip-feat/x` too, so anchor it with `^` and `$` to match the whole name;
- it is shorter than `min-length` characters, when that option is set;
- it is longer than `max-length` characters, when that option is set.

Main passes: a name one of the references `main` of `[plugins.git] options` names
(`refs/remotes/origin/main` and `refs/heads/main` both name `main`). So does a repository with no
commit yet.

Scope: repository. Reads history: yes (the branch and the push, through the host's git interface).
Aspects: none.

## Options

The rule reads `main` from `[plugins.git] options`, and has its own options:

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `pattern` | string (regex) | none; required | Matches an allowed branch name; searched anywhere in it unless anchored with `^…$`. |
| `min-length` | integer | none | The shortest allowed name, in characters. |
| `max-length` | integer | none | The longest allowed name, in characters; a `min-length` above it is refused. |

```toml
[plugins.git]
source = "ElsiKora/Gate:git@0.1.0"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]

[rules]
"git/branch-name" = { level = "error", options = { pattern = '^(feat|fix|docs|chore)/[a-z0-9-]+$', min-length = 5, max-length = 60 } }
```

## Example finding

Pushing to, or working on, the branch `wip`:

```text
error git/branch-name commit 3f2a9c1d0b7e5a4c8d6f1e2b3a4c5d6e7f8a9b0c
  branch wip does not match the pattern "^(feat|fix|docs|chore)/[a-z0-9-]+$"
  help: rename the branch to a name the policy allows: git branch -m <new-name>, then push it under
  the new name
```

The finding points at the pushed tip under `--push`, else at the commit `HEAD` is on; its target is
the branch name.

## How to fix it

Rename the branch, and push it under the new name:

```bash
git branch -m feat/short-description
git push -u origin feat/short-description
```

A branch that keeps its name, such as a long-lived release branch, gets an exception by its name:

```toml
[[exception]]
rule = "git/branch-name"
target = "release-2026"
reason = "the release branch of the 2026 line"
```

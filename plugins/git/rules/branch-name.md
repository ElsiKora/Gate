# `git/branch-name`

The branch a push updates, or else the checked-out branch, has a name the policy allows: a
placeholder template, as [`@elsikora/git-branch-lint`](https://github.com/ElsiKora/git-branch-lint)
checks it, or a regular expression.

## What it checks and why

A branch name is how a change is found again: in a merge subject, a pull request, a CI run. A name
outside the convention is caught while the branch is still cheap to rename, not after it has been
merged into main's history. Which name the rule checks depends on the run:

- **A run under `--push`** (the pre-push hook): the branch each pushed ref updates. `git push origin
  HEAD:bad` checks `bad`, whatever branch is checked out. A pushed ref outside `refs/heads/` (a tag)
  updates no branch, and is not checked.
- **Any other run** (`gate check`, the pre-commit and commit-msg hooks, `--rev`): the checked-out
  branch, as the host reports it.
- **A detached `HEAD`** (a CI checkout of a commit or of a pull request's merge): no branch is
  checked out, and the rule checks nothing.

The checks run in the reference's order, and the first one that fails is reported:

1. **prohibited**: in a run a git hook starts, a name listed in `prohibited` is refused outright:
   by default `main`, `master` and `release`, so no one commits or pushes to them from a working
   copy. The generated hooks pass `--hook` to every `check`; a plain `gate check`, and a CI job,
   which checks main out and checks a push to main, do not, and there the list does not apply.
2. **ignore**: a name listed in `ignore` (by default `dev`) passes every other check.
3. **min-length** and **max-length**: the name is at least `min-length` and at most `max-length`
   long (by default 5 and 50); 0 turns a limit off. A length is counted as the reference counts it,
   in UTF-16 code units: a character outside the Basic Multilingual Plane, such as an emoji, counts
   twice.
4. **the name**: the whole name matches `template`, or `pattern` matches it.

Outside a hook run, main passes: a name one of the references `main` of `[plugins.git] options`
names (`refs/remotes/origin/main` and `refs/heads/main` both name `main`). So does a repository with
no commit yet.

### Templates

A template is the reference's: placeholders `:name` (a letter, then letters, digits and `-`, not
ending in `-`) in literal text. `:type` is one of `types`; `:ticket` is two or more lower-case
letters, a dash and digits (`proj-123`); every other placeholder takes `subject`: one regular
expression for all (by default `[a-z0-9-]+`), or a table with one per placeholder name, the others
keeping the default. A table entry for `ticket` replaces its pattern; `:type` always takes `types`
(the default subject when `types` is empty).

A placeholder written with a `-` after it (`:ticket-`) is optional. The name may match the template
with any set of the optional placeholders left out; where one is left out, a delimiter repeated
where it was (`//`, `--`, `__`, `..`) is collapsed to one, and delimiters left at either end are
removed. So `:type/:ticket-:name` takes `feature/proj-12-login` and `feature/login`. The template's
other characters are regular-expression source, as in the reference: `.` matches any character
but a line terminator.

`template`, `pattern` and `subject` mean what they mean in the reference, a JavaScript regular
expression without flags, read in the subset that
[`git/commit-message`](commit-message.md#regular-expressions) lists for its patterns: `\w`, `\d`
and `\b` (and `\W`, `\D`, `\B`) are ASCII, so `subject = "[\\w-]+"` takes `cafe` and refuses
`café`, and syntax of Rust's `regex` crate such as `\A` or `[[:alpha:]]` means what JavaScript
means by it.

A template and a pattern together, or neither, are refused when the configuration loads.

Scope: repository. Reads history: yes (the branch, the push and whether a hook runs, through the
host's git interface). Aspects: none.

## Options

The rule reads `main` from `[plugins.git] options`, and has its own options. Exactly one of
`template` and `pattern` is given; the others default to the reference's defaults, so
`template = ":type/:ticket-:name"` alone is its default policy.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `template` | string | none | A placeholder template the whole name matches. |
| `pattern` | string (regex) | none | Matches an allowed branch name; searched anywhere in it unless anchored with `^…$`. |
| `types` | list of strings | `["bugfix", "feature", "hotfix", "release", "support"]` | The values of `:type`. |
| `subject` | string (regex), or a table of them by placeholder name | `"[a-z0-9-]+"` | The pattern of the other placeholders. |
| `prohibited` | list of strings | `["main", "master", "release"]` | Names refused in a hook run. |
| `ignore` | list of strings | `["dev"]` | Names that pass every check but `prohibited`. |
| `min-length` | integer | `5` | The shortest name, in UTF-16 code units; 0 for no limit. |
| `max-length` | integer | `50` | The longest name, in UTF-16 code units; 0 for no limit. A `min-length` above it is refused. |

```toml
[plugins.git]
source = "ElsiKora/Gate:git@0.1.0"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]

[rules]
"git/branch-name" = { level = "error", options = { template = ":type/:ticket-:name", types = ["feature", "bugfix", "hotfix", "release"], subject = { name = "[a-z0-9]+(?:-[a-z0-9]+)*" } } }
```

## Example findings

Working on, or pushing, the branch `wip/login`:

```text
error git/branch-name commit 3f2a9c1d0b7e5a4c8d6f1e2b3a4c5d6e7f8a9b0c
  branch wip/login does not match the template ":type/:ticket-:name"; valid branch types: feature,
  bugfix, hotfix, release
  help: rename the branch: git branch -m <new-name>, then push it under the new name
```

Committing on `main` (the commit-msg hook runs `check --hook`):

```text
error git/branch-name commit 3f2a9c1d0b7e5a4c8d6f1e2b3a4c5d6e7f8a9b0c
  branch main is prohibited; prohibited branch names: main, master, release
  help: work on a branch of its own: git switch -c <new-name>; or rename the branch: git branch -m
  <new-name>, then push it under the new name
```

The finding points at the pushed tip under `--push`, else at the commit `HEAD` is on; its target is
the branch name.

## How it differs from the reference

The rule gives the verdicts of `@elsikora/git-branch-lint` 1.2.2 (commit 9718c1a), except:

- The reference lints `git rev-parse --abbrev-ref HEAD`, which is `HEAD` on a detached head; the rule
  checks nothing there.
- `prohibited` applies only in a hook run, and main passes outside one; the reference has no notion
  of the run and refuses `main` in CI too.
- The patterns are read in that subset: lookaround, backreferences and the other constructs it
  leaves out are a configuration error that names the construct.
- A `$` in `subject` or `types` is taken literally; the reference reads `$&`, `` $` `` and `$'` in
  them as JavaScript replacement patterns.

## How to fix it

Rename the branch, and push it under the new name:

```bash
git branch -m feature/proj-12-short-description
git push -u origin feature/proj-12-short-description
```

A branch that keeps its name, such as a long-lived support branch, is listed in `ignore`, or gets an
exception by its name:

```toml
[[exception]]
rule = "git/branch-name"
target = "support-2026"
reason = "the support branch of the 2026 line"
```

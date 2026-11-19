# `git/main-history`

Every first-parent commit of main since activation follows the git policy.

## What it checks and why

Main holds a few kinds of commits, and each has to be the kind it says it is: by default a merge of
reviewed work, a release, or a documentation fix. A commit that is code but didn't come in the way the
policy says skipped the checks that way has. The rule walks the first-parent line of main from the
activation commit (not included) to the tip of main. Main is the pushed tip when the run checks a push
(`--push`) that updates one of the `main` references (`refs/remotes/origin/main` and
`refs/heads/main` both name the branch `main`); otherwise it is the first reference of `main` that
exists.

Each commit is classified by the policy's forms, `forms` of `[plugins.git] options`, tried in order:
the first form whose parent count and subject match is the commit's form. The forms are shared with
`git/no-ci-skip`, which exempts the commits of a release form. A form has:

- a `kind`, `merge`, `release` or `direct`: the aspect of its findings and the checks it can run;
- an optional `parents`, the parent count it matches; any count when absent;
- an optional `subject`, a regular expression the commit's subject (its first line) matches; any
  subject when absent. A release form takes no `subject`: it matches the commits whose whole trimmed
  message matches `release-subject`;
- an optional `checks`, the checks a commit of the form passes; every check of its kind when absent,
  none when `[]`.

A commit that has none of the forms is reported, under `#merge` when it has two parents or more and
`#direct` otherwise.

The checks:

- **`merge`: `body`.** No non-blank line after the subject: git's message, unedited.
- **`merge`: `branch`.** Capture 1 of the form's `subject` is the merged branch. Its name must match
  `branch-pattern` and have a length within `branch-length`, or start with `dependabot-prefix`; a
  Dependabot branch may change only the files `dependabot` matches. The form's `subject` needs a
  capture group; a subject that matches with group 1 empty (an alternative without it) names no
  branch, and is reported.
- **`merge`: `sync`.** The second parent is not already on main (a sync merge). Needs `parents = 2`.
- **`merge`: `tree`.** The merge's tree equals git's conflict-free merge of its parents (`git
  merge-tree --write-tree`); a merge whose parents conflict is not compared. Needs `parents = 2`.
- **`release`: `files`.** It may change only `release-files`. Its `package-lock.json` may change only
  the top-level and root-package `version` fields, and its `package.json` only `version`, which must
  be the version capture 1 of `release-subject` names. When either file it changes does not read as
  JSON on one side (absent, not UTF-8 or not JSON), the commit is reported. Needs `parents = 1`.
- **`direct`: `files`.** The host says what `gate sync` produces at that commit (its generators'
  outputs, the tracked files under a glob a generator owns, the core's hook and CI files) and which of
  it differs there (the plugin interface's `sync-state`). At a commit from before the repository
  moved to Gate, whose `gate.toml` is not a configuration of this core, the host answers the run's
  produced paths and no drift: content another tool wrote is not judged, only paths count. Needs
  `parents = 1`. For each file the commit changes:
  1. one that differs from `gate sync` is reported: a generated file that is not its generator's
     output, a tracked file under an owned glob that no generator writes, a hand-edited hook or CI
     file;
  2. one `gate sync` produces and that is in sync is allowed: it changes with its sources. A path
     the commit removes (a deletion, or the old side of a rename: changed paths are listed without
     rename detection) is judged against what `gate sync` produced at the parent, so a generated
     file that goes with its sources is allowed;
  3. one `documentation` matches is allowed, except a change to the Enforcement/Status line of a
     rule in a rule file under `rules-directory`, and a rule file there that is not UTF-8 text,
     whose lines cannot be compared;
  4. any other file is a code path, and is reported.

The default forms, when `[plugins.git] options` has no `forms`, are the merge, release and direct
policy:

```toml
forms = [
  { kind = "merge", parents = 2, subject = "^Merge branch '([^']+)'$" },
  { kind = "release", parents = 1 },
  { kind = "direct", parents = 1 },
]
```

So a two-parent commit must be git's unedited merge of a well-named branch, a one-parent commit
whose message is the release subject a clean release, and any other one-parent commit a
documentation change; a merge of three parents or more has no form.

A form that cannot run its checks is refused before any commit is read: a check its kind does not
run, `branch` without a capture group in `subject`, `sync` or `tree` without `parents = 2`, `files`
without `parents = 1`, a release form with a `subject`, an empty `forms`.

Every pattern (`subject`, `release-subject`, `branch-pattern`) is a regular expression searched
anywhere in its text: `^Merge ` matches only at the start, `Merge ` anywhere in the subject. Anchor a
pattern with `^` and `$` to match the whole text. Globs are matched against repository paths; `*`
does not cross `/`. The rule reads history only through the host's git
interface and needs the full history: in a shallow clone the activation commit is missing and the
run fails with an error that says so.

Scope: repository. Reads history: yes. Aspects: `direct`, `merge`, `release`.

## Options

The shared policy is set once in `[plugins.git] options`. `main` is required; each other key is
required by what reads it, and the error names the reader:

| Key | Type | Meaning | Read by |
| --- | --- | --- | --- |
| `main` | array of string | References of main, tried in order; the first that names a commit is main. | every git rule |
| `activation` | string | The full 40-hex id of the commit after which the policy holds. | this rule, `git/no-ci-skip` |
| `release-subject` | string (regex) | Matches a release commit's whole trimmed message; capture 1 is the version; a pattern without a capture group is refused. | a release form |
| `branch-pattern` | string (regex) | Matches a merged branch's name. | the `branch` check |
| `branch-length` | `[min, max]` | The length range of a merged branch's name, in characters; a minimum above the maximum is refused. | the `branch` check |
| `documentation` | array of glob | Files a direct commit may change. | the direct `files` check |
| `dependabot` | array of glob | Files a Dependabot branch may change. | the `branch` check |
| `forms` | array of form | The forms a commit on main may have, tried in order; the three forms above when absent. | this rule, `git/no-ci-skip` |

The plugin declares these keys as the schema of its options, so Gate checks them when the plugin
loads: a missing `main`, an unknown key (in a form too) or a value of the wrong type is a
configuration error before anything runs. So is a key the table names as read by an enabled rule,
form or check: the rules declare the keys they read, and under which forms, so the error names the
rule and the key (`[plugins.git] options has no activation, which "git/no-ci-skip" reads: …`).

The rule's own options:

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `dependabot-prefix` | string | `"dependabot/"` | The branch prefix of Dependabot's branches. |
| `release-files` | array of string | `["package.json", "package-lock.json", "CHANGELOG.md"]` | The files a release commit may change. |
| `rules-directory` | string | `"docs/agents/rules"` | Where the agent rule files are. |

The default policy:

```toml
[plugins.git]
source = "ElsiKora/Gate:git@0.1.0"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "0123456789abcdef0123456789abcdef01234567"
release-subject = '^chore\(release\): (\d+\.\d+\.\d+) \[skip ci\]$'
branch-pattern = '^(feat|fix|docs|chore)/[a-z0-9-]+$'
branch-length = [5, 60]
documentation = ["docs/**", "*.md"]
dependabot = ["package.json", "package-lock.json"]

[rules]
"git/main-history" = "error"
```

A subject-based policy: every commit on main has a Conventional Commits subject or is a revert, and
merges keep a `Merge ...` subject. No check beyond the form runs and there is no release form, so
only `main`, `activation` and `forms` are needed; `git/no-ci-skip` then exempts no commit:

```toml
[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "0123456789abcdef0123456789abcdef01234567"

[[plugins.git.options.forms]]
kind = "merge"
parents = 2
subject = '^Merge '
checks = []

[[plugins.git.options.forms]]
kind = "direct"
parents = 1
subject = '^((feat|fix|docs|chore|refactor|test|ci|build|perf|style)(\([a-z0-9-]+\))?!?: .+|Revert ".+")$'
checks = []

[rules]
"git/main-history" = "error"
```

## Example finding

A direct commit on main that changes `src/main.ts`:

```text
error git/main-history#direct commit 3f2a9c1d0b7e5a4c8d6f1e2b3a4c5d6e7f8a9b0c
  direct commit 3f2a9c1d0b7e changes code path src/main.ts; code reaches main through a branch merge
  help: a direct commit to main may change documentation only; move the change to a branch and
  merge it
```

The finding's target is the commit's full id.

## How to fix it

History on main is not rewritten. A commit already on main gets an exception, which records why it
stands:

```toml
[[exception]]
rule = "git/main-history#direct"
target = "3f2a9c1d0b7e5a4c8d6f1e2b3a4c5d6e7f8a9b0c"
reason = "hotfix pushed during the outage; reviewed afterwards"
```

Before it reaches main: put the change on a branch named by the policy and merge it with
`git merge --no-ff`, keeping git's message; resolve conflicts on the branch.

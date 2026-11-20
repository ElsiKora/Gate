# `git/no-ci-skip`

No commit since activation carries a CI-skip directive except a release commit.

## What it checks and why

GitHub Actions skips the push and pull-request workflows of a commit whose message holds
`[skip ci]`, `[ci skip]`, `[no ci]`, `[skip actions]` or `[actions skip]`, or the trailer
`skip-checks: true`. A commit that skipped CI reached main unchecked. The rule reads every commit
of `activation..main` (the merged branches' commits too) and the snapshot's commits that main
doesn't have yet, and reports each one whose message holds a directive (`directive-pattern`) or
the trailer (`trailer-pattern`).

A release commit is exempt: a commit on main's first-parent line whose form is a release form of
`[plugins.git] options forms` (see [`git/main-history`](main-history.md)): by default a
single-parent commit whose whole message matches `release-subject`. Release tooling marks it to skip
CI because CI made it. Forms without a release form exempt no commit, and a commit an earlier form
takes is not a release. The patterns are searched anywhere in the message.

The message being written is checked too, before the commit exists: the `commit-msg` hook passes it
(`gate check --commit-message <file>`), and the rule reports a directive anywhere above the
scissors line that `git commit --verbose` writes. `#` lines count: git keeps them in a message given
with `-m` or `-F` (`git commit -m "feat(core): add x" -m "#12 done [skip ci]"` stores the directive),
so the rule searches them too; [`git/commit-message`](commit-message.md) skips them, as commitlint
does. A message whose whole text matches `release-subject` is exempt when the forms hold a release
form.

Scope: repository. Reads history: yes. Aspects: none.

## Options

`main`, `activation`, `forms` and, for a release form, `release-subject` are the shared policy in
`[plugins.git] options` (see [`git/main-history`](main-history.md)). The rule's own options:

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `directive-pattern` | string (regex) | the five directives in any case | The skip directives. |
| `trailer-pattern` | string (regex) | `skip-checks: true` in any case, on a line of its own | The skip trailer. |

Both are read as JavaScript reads a regular expression without flags, as every pattern option of
the git plugin is (see [Regular expressions](commit-message.md#regular-expressions)). Without flags
there is no `i` or `m`, so the defaults spell each letter in both cases and anchor the trailer at a
line break:

```toml
directive-pattern = '\[(?:[sS][kK][iI][pP] [cC][iI]|[cC][iI] [sS][kK][iI][pP]|[nN][oO] [cC][iI]|[sS][kK][iI][pP] [aA][cC][tT][iI][oO][nN][sS]|[aA][cC][tT][iI][oO][nN][sS] [sS][kK][iI][pP])\]'
trailer-pattern = '(?:^|\n)[sS][kK][iI][pP]-[cC][hH][eE][cC][kK][sS]:\s*[tT][rR][uU][eE]\s*(?:\n|$)'
```

```toml
[rules]
"git/no-ci-skip" = "error"
```

## Example finding

```text
error git/no-ci-skip commit 7c1e0f9a2b3d4c5e6f7a8b9c0d1e2f3a4b5c6d7e
  7c1e0f9a2b3d carries the forbidden CI-skip directive [skip ci]
  help: reword the commit without the directive (an unpushed commit: git commit --amend or git
  rebase -i); a skipped run leaves main unchecked
```

The finding's target is the commit's full id. In the `commit-msg` hook:

```text
error git/no-ci-skip
  the commit message carries the forbidden CI-skip directive [skip ci]
  help: reword the message without the directive; a skipped run leaves main unchecked
```

There the finding has no location, since the commit does not exist yet; its target is the message's
first line.

## How to fix it

Before pushing, reword the commit (`git commit --amend` for the last one, `git rebase -i` for older
ones). A commit already on main gets an exception with its full id as the target.

A run judges its whole history, each commit id an exception names, and in the `commit-msg` hook the
first line of the message being written. An excepted id outside the history (before activation, or
rewritten) has no finding, so its exception is reported; an exception keyed by a message's first
line is due only in the `commit-msg` run that writes that message.

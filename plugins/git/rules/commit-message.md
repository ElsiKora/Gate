# `git/commit-message`

The message of the commit being made has the conventional form, `<type>(<scope>): <subject>`, as
[commitlint](https://commitlint.js.org) checks it with `@commitlint/config-conventional` and the
rule levels of [`@elsikora/git-branch-lint`](https://github.com/ElsiKora/git-branch-lint)'s
`commitlint.config.js`.

## What it checks and why

A history whose headers say what kind of change each commit is, and where, can be read, searched and
turned into release notes. The message is checked in the `commit-msg` hook, before the commit
exists, when rewording it costs nothing:

```toml
[hooks]
commit-msg = ["check --commit-message"]
```

The hook passes the message file to `gate check --commit-message <file>`. A run without
`--commit-message` checks nothing, and neither does a blank message or one with only comment lines.

The message is read as `commitlint --edit` reads it:

- Lines starting with `#` are comments, and the line `# ------------------------ >8
  ------------------------` (which `git commit --verbose` writes) ends the message.
- Merges (`Merge branch …`, `Merge pull request …`, `Merge <a> into <b>`, `Merge tag …`,
  `Merge remote-tracking branch …`, `Merged … in …`, `Merged PR …: …`, `Automatic merge…`,
  `Auto-merged … into …`), reverts (`Revert …`, `Reapply …`), `amend!`, `fixup!` and `squash!`
  commits, and a release whose header is a version (`chore(release): 1.2.0 [skip ci]`, `v1.2.3`) are
  not checked.
- The first line is the header: `type(scope): subject`, with a `!` before the colon for a breaking
  change. The lines after it are the body, until a line with an issue reference (`Closes #12`, `see
  #12`) or a `BREAKING CHANGE:` note, which starts the footer.

Each check is an aspect named after the commitlint rule it ports, and is reported on its own:

| Aspect | Fails when |
| --- | --- |
| `header-trim` | The header starts or ends with whitespace. |
| `header-max-length` | The header is longer than `header-max-length`, or there is no header line. |
| `header-min-length` | The header is shorter than `header-min-length`, or there is no header line. |
| `header-case` | The header starts with a letter and is not lower case. |
| `header-full-stop` | The header ends with `.`. |
| `type-empty` | The header names no type. |
| `type-case` | The type is not lower case. |
| `type-enum` | The type is not one of `types`. |
| `scope-empty` | The header names no scope. |
| `scope-case` | A scope is not lower case; `a/b`, `a\b` and `a, b` are several scopes. |
| `scope-max-length` | The scope is longer than `scope-max-length`. |
| `subject-empty` | The header has no subject. |
| `subject-case` | The subject starts with a letter that has a case and is not lower case. |
| `subject-full-stop` | The header ends with `.`, but not with `...`. |
| `subject-min-length` | The subject is shorter than `subject-min-length`. |
| `subject-max-length` | The subject is longer than `subject-max-length`. |
| `body-leading-blank` | There is a body, and the message's second line is not blank. |
| `body-max-line-length` | A body line without a URL is longer than `body-max-line-length`. |
| `body-full-stop` | The body does not end with `.`. |
| `footer-leading-blank` | There is a footer, and the line before it is not blank. |
| `footer-max-line-length` | A footer line without a URL is longer than `footer-max-line-length`. |

A case check skips text in backquotes or quotes (`` add the `README` `` is lower case), and passes
text that starts with a digit.

The reference reports `header-min-length` and `scope-max-length` as warnings and the others as
errors. Give the two aspects that level in the rule's entry:

```toml
[rules]
"git/commit-message" = { level = "error", levels = { header-min-length = "warning", scope-max-length = "warning" } }
```

Scope: repository. Reads history: yes (the message, through the host's git interface). Aspects: the
21 above.

## Options

Every option defaults to the reference's value. A length is counted in characters; 0 turns a
longest-length check off.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `types` | list of strings | `["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert", "wip"]` | The types a header may name. |
| `header-max-length` | integer | `100` | The longest header. |
| `header-min-length` | integer | `10` | The shortest header. |
| `scope-max-length` | integer | `30` | The longest scope. |
| `subject-max-length` | integer | `80` | The longest subject. |
| `subject-min-length` | integer | `3` | The shortest subject. |
| `body-max-line-length` | integer | `100` | The longest body line. |
| `footer-max-line-length` | integer | `100` | The longest footer line. |

## Example finding

Committing with the message `feature(auth): add the login form`:

```text
error git/commit-message#type-enum
  type feature is not a commit type; the commit types: feat, fix, docs, style, refactor, perf, test,
  build, ci, chore, revert, wip
  help: write the header as <type>(<scope>): <subject>, in lower case, with a type of feat, fix,
  docs, style, refactor, perf, test, build, ci, chore, revert, wip
```

The finding has no location, since the commit does not exist yet; its target is the header.

## How it differs from the reference

Measured on the same messages against `@commitlint/cli` 20.5.3:

- Lengths are counted in characters, where the reference counts UTF-16 code units: a character
  outside the Basic Multilingual Plane, such as an emoji, counts once here and twice there.
- The comment character is always `#`; the reference reads git's `core.commentChar`.
- A blank message passes; the reference stops with `[input] is required`. Git refuses an empty
  message on its own.

## How to fix it

Reword the message and commit again; a commit that already exists is reworded with
`git commit --amend` (the last one) or `git rebase -i` (an older one). A message with a type the
project uses beyond the reference's twelve gets it in `types`.

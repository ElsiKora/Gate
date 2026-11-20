# `git/commit-message`

The messages of the commit being made and of the commits a push sends have the conventional form,
`<type>(<scope>): <subject>`, as [commitlint](https://commitlint.js.org) checks it with
`@commitlint/config-conventional` and the rule levels of
[`@elsikora/git-branch-lint`](https://github.com/ElsiKora/git-branch-lint)'s `commitlint.config.js`.

## What it checks and why

A history whose headers say what kind of change each commit is, and where, can be read, searched and
turned into release notes. The message is checked in the `commit-msg` hook, before the commit
exists, when rewording it costs nothing:

```toml
[hooks]
commit-msg = ["check --commit-message"]
```

The hook passes the message file to `gate check --commit-message <file>`. A blank message, or one
with only comment lines, is not checked.

A commit made without that hook (`git commit -n`, a cherry-pick, a rebase, an edit in a web
interface) is checked when it is pushed: a `gate check --push` run, which the `pre-push` hook and
the CI job start, also checks the message of each commit the push adds to the branch, oldest first,
with every check and option below. Those are the commits reachable from the pushed head and neither
from the remote ref's old tip nor from main, the first reference of `[plugins.git] options main`
that names a commit:

- A push to an existing branch checks its new commits only. After a merge of main into the branch,
  or a rebase onto main and a force push, the commits main already holds (a web edit, a bot commit)
  are not checked: the developer cannot reword them.
- A new branch checks every commit it adds to main (`main..head`), the commits `git/no-ci-skip`
  reads for it.
- A push whose target is not a branch (`refs/heads/…`), such as a tag, checks nothing.

A finding of a pushed commit is on that commit, starts with its short id, and names the commit as
its target. Its help says to reword the commit with `git rebase -i <id>^` and push the branch again,
or, when others already have it, to add an exception for its id. A `--rev` run, and a run with
neither `--commit-message` nor `--push`, check nothing.

The message is read as `commitlint --edit` reads it:

- Lines starting with `#` are comments, and the line `# ------------------------ >8
  ------------------------` (which `git commit --verbose` writes) ends the message.
- Merges (`Merge branch …`, `Merge pull request …`, `Merge <a> into <b>`, `Merge tag …`,
  `Merge remote-tracking branch …`, `Merged … in …`, `Merged PR …: …`, `Automatic merge…`,
  `Auto-merged … into …`), reverts (`Revert …`, `Reapply …`), `amend!`, `fixup!` and `squash!`
  commits, and a release whose header is a version (`chore(release): 1.2.0 [skip ci]`, `v1.2.3`) are
  not checked: commitlint's default ignores. `default-ignores = false` checks them too, and a message
  one of `ignores` matches is not checked either.
- The first line is the header: `type(scope): subject`, with a `!` before the colon for a breaking
  change. A custom `header-pattern` alone decides the header, so the `!` is the pattern's to allow;
  without one, a breaking header is read with the conventional breaking pattern first, as the
  reference reads it. The lines after it are the body, until a line with an issue reference
  (`Closes #12`, `see
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
| `body-full-stop` | The body, above its trailer block, does not end with `.`; a body that is only a trailer block passes. |
| `footer-leading-blank` | There is a footer, and the line before it is not blank. |
| `footer-max-line-length` | A footer line without a URL is longer than `footer-max-line-length`. |
| `prohibited-body-lines` | A body line matches one of `prohibited-body-lines`; one finding per line. |
| `prohibited-trailers` | A trailer of the message's trailer block matches one of `prohibited-trailers`; one finding per trailer. |

A case check skips text in backquotes or quotes (`` add the `README` `` is lower case), and passes
text that starts with a digit.

The reference reports `header-min-length` and `scope-max-length` as warnings and the others as
errors. The plugin's preset `git/recommended` gives the two aspects that level (and turns on
[`git/branch-name`](branch-name.md) with the reference's default template). The project's own
configuration wins: a level it gives the whole rule (`"git/commit-message" = "note"`) applies to
these two aspects too, and a level it gives an aspect wins over both:

```toml
[project]
extends = ["git/recommended"]
```

A repository whose root is not a project (a `[workspace]` root holds no `extends`) writes the levels
in the rule's entry instead:

```toml
[rules]
"git/commit-message" = { level = "error", levels = { header-min-length = "warning", scope-max-length = "warning" } }
```

Scope: repository. Reads history: yes (the message, through the host's git interface). Aspects: the
23 above.

## Options

Every option defaults to the reference's value. A length is counted as the reference counts it, in
UTF-16 code units: a character outside the Basic Multilingual Plane, such as an emoji, counts twice.
0 turns a longest-length check off.

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
| `header-pattern` | string | the conventional header | The header's regular expression, commitlint's `headerPattern`, with the named groups `type`, `scope` and `subject`. |
| `ignores` | list of strings | `[]` | Regular expressions of messages that are not checked, besides the default ignores. |
| `default-ignores` | boolean | `true` | Whether commitlint's default ignores apply. |
| `prohibited-trailers` | list of strings | `[]` | Regular expressions of the trailers the message being written may not carry. |
| `prohibited-body-lines` | list of strings | `[]` | Regular expressions of the body lines the message being written may not carry. |

### Regular expressions

`header-pattern`, `ignores`, `prohibited-trailers` and `prohibited-body-lines` are read as JavaScript reads a regular
expression without flags, which is how the reference reads the first two. Every pattern option of
the git plugin is read the same way:
`template`, `pattern` and `subject` of [`git/branch-name`](branch-name.md), `directive-pattern` and
`trailer-pattern` of [`git/no-ci-skip`](no-ci-skip.md), and `release-subject`, `branch-pattern` and
a form's `subject` in `[plugins.git] options` (see [`git/main-history`](main-history.md)). The
plugin reads this subset of that syntax:

- Characters, `.`, `^`, `$`, `|`, the quantifiers `*`, `+`, `?`, `{n}`, `{n,}` and `{n,m}`, each
  with a lazy `?`, the groups `(...)`, `(?:...)` and `(?<name>...)`, and classes `[...]` and
  `[^...]` with ranges.
- The escapes `\d`, `\D`, `\w`, `\W`, `\s`, `\S`, `\b`, `\B`, `\n`, `\r`, `\t`, `\v`, `\f`, `\0`,
  `\xHH`, `\uHHHH` (a surrogate pair included) and `\cA` to `\cZ`. `\w`, `\d` and `\b` are ASCII,
  `\s` is JavaScript's white space, and `.` matches no line terminator.
- Any other escaped character is that character: `\.`, `\<`, `\A`, `\z` and `\pL` match `.`, `<`,
  `A`, `z` and `pL`. Inside a class every character is literal, so `[[:alpha:]]`, `&&`, `--` and
  `~~` are characters there, as in JavaScript, and a `{` that does not make a quantifier is a `{`.
- Not read, and an error that names the construct and the option: lookaround (`(?=`, `(?!`,
  `(?<=`, `(?<!`), backreferences (`\1`, `\k<name>`), octal escapes, other `(?` groups such as
  inline flags, the empty classes `[]` and `[^]`, and a lone surrogate.
- A character outside the Basic Multilingual Plane, such as an emoji, is one character to `.` and
  to a class, where JavaScript without the `u` flag sees two.

commitlint's `headerPattern` names its groups by position, through `headerCorrespondence`; here
the groups are named. The conventional preset's pattern and the form this option takes:

```text
commitlint:     ^(\w*)(?:\((.*)\))?!?: (.*)$   with headerCorrespondence type, scope, subject
header-pattern: ^(?<type>\w*)(?:\((?<scope>.*)\))?!?: (?<subject>.*)$
```

A header the pattern does not match names no type, so `type-empty` fails. A pattern with no
`subject` group names no subject, so `subject-empty` fails on every message, as in commitlint. A
`header-pattern` that does not compile, or has no `type` group, is an error that names the option
and fails the run.

commitlint's `ignores` are functions of the message. Here each one is a regular expression, which
is this rule's mapping of them: it is searched in the whole message as the hook gives it, body and
comment lines included, so `WIP` skips `feat: x` with `WIP` in its body. `^` anchors at the start
of the message only, not at each line: `^WIP` skips `WIP: x` but not a body line `WIP`.

A policy written as one header expression, with its own messages to skip:

```toml
[rules]
"git/commit-message" = { level = "error", options = { types = ["feat", "fix", "docs", "chore"], header-pattern = '^(?<type>\w*)(?:\((?<scope>[a-z0-9-]+(?:,[a-z0-9-]+)*)\))?!?: (?<subject>.+)$', ignores = ["^Merge ", '^Revert "'], default-ignores = false, subject-min-length = 0 }, levels = { scope-empty = "off", subject-case = "off" } }
```

With it, `feat(api,web): add x` passes and `feat(api_web): add x` fails `type-empty`; `Merge commit
'abc'` is skipped, and `fixup! feat: x` is checked and fails.

### Prohibited trailers

`prohibited-trailers` refuses trailers, the `Token: value` lines at the end of a message, such as a
co-author a repository does not accept; commitlint can require a trailer (`trailer-exists`) but
not refuse one. Like every check of this rule it reads the message of the commit being made and
the messages of the commits a push sends; a commit already on the remote ref is not checked. The
trailers are found as `git interpret-trailers` finds them:

- They are the message's last paragraph, never its first (the title), with comment lines, a
  trailing old-style `Conflicts:` block (`Conflicts:` and the tab-indented paths after it) and the
  scissors section left out.
- That paragraph is a trailer block when each of its lines is a trailer or continues the one above
  it (starts with white space), or when it holds a line git writes itself (`Signed-off-by: `,
  `(cherry picked from commit `) and at least a quarter of its lines are trailers. A trailer's
  token is ASCII letters, digits and `-`, and may have spaces before its colon.
- Each trailer is matched unfolded, as `Token: value`: the token, a colon and a space, and the
  value with its continuation lines joined by a space.

Each pattern is searched in each trailer, and every trailer one matches is a finding that names it.
The same text elsewhere in the message, in the body or a paragraph above the last, is not a
trailer and passes. An ignored message (a merge, a `fixup!`) is checked too, since a trailer stays
in history whatever the header says. A pattern has no flags, so a token is spelled
in both cases where both are expected:

```toml
[rules]
"git/commit-message" = { level = "error", options = { prohibited-trailers = ['^[Cc]o-[Aa]uthored-[Bb]y: .*\b[Aa]gent\b'] } }
```

With it, a message ending in `Co-Authored-By: Example Agent` fails, one ending in
`Co-authored-by: A Person` passes, and so does a body line `Co-authored-by: Example Agent` with
prose after it. A pattern that does not compile, and an empty one, which would refuse every
trailer, are errors that name the option and fail the run.

The trailer block is also where `body-full-stop` stops reading: the body is judged above it, so a
body that ends with a full stop and then a `Co-authored-by: A Person` block passes, and a body
that is only a trailer block has no prose to end. `body-max-line-length` still reads every line.

### Prohibited body lines

`prohibited-body-lines` refuses lines of the body, such as a note that a tool or an assistant wrote
the message, which is no trailer and which `prohibited-trailers` therefore does not see. The body
lines are the lines after the header that are not blank and not in the trailer block, found as
above: comment lines, a trailing `Conflicts:` block and the scissors section are not part of the
message, and the second line counts even when no blank line comes before it. Each pattern is
searched in each body line as written, and every line one matches is a finding that names it. An
ignored message is checked too. The header and the trailers are not body lines; a trailer is
`prohibited-trailers`' to refuse:

```toml
[rules]
"git/commit-message" = { level = "error", options = { prohibited-body-lines = ['^[Gg]enerated (with|by) '], prohibited-trailers = ['^[Gg]enerated-([Ww]ith|[Bb]y): '] } }
```

With it, a body line `Generated with an assistant.` fails `prohibited-body-lines`, a trailer
`Generated-with: an assistant` fails `prohibited-trailers`, and a line `A generator writes it.`
passes. A pattern that does not compile, and an empty one, which would refuse every body line, are
errors that name the option and fail the run.

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

Pushing a commit `a1b2c3d4e5f6…` made with `git commit -n` and the same message:

```text
error git/commit-message#type-enum commit a1b2c3d4e5f60718293a4b5c6d7e8f9012345678
  a1b2c3d4e5f6: type feature is not a commit type; the commit types: feat, fix, docs, style,
  refactor, perf, test, build, ci, chore, revert, wip
  help: write the header as <type>(<scope>): <subject>, in lower case, with a type of feat, fix,
  docs, style, refactor, perf, test, build, ci, chore, revert, wip
```

Its target, and its key, is the commit id.

## How it differs from the reference

The rule gives the verdicts of `@commitlint/cli` 20.5.3 with that configuration, except:

- The comment character is always `#`; the reference reads git's `core.commentChar`.
- A push checks `<base>..<head>`, as `commitlint --from <base> --to <head>` does; for a new branch,
  with no base, it checks the head commit alone, where the reference needs a `--from`.
- `body-full-stop` judges the body above its trailer block; the reference reads trailers as body,
  so a message whose last paragraph is `Co-authored-by: A Person` fails it there.
- `prohibited-body-lines` has no counterpart in the reference.
- `prohibited-trailers` has no counterpart in the reference. It finds the trailer block as git does
  without configuration: the tokens of `trailer.<token>.key` do not count as lines git writes, and
  the separator is always `:`.

## How to fix it

Reword the message and commit again; a commit that already exists is reworded with
`git commit --amend` (the last one) or `git rebase -i` (an older one) before it is pushed. A commit
already shared, which cannot be reworded, gets an exception by its commit id. A message with a type
the project uses beyond the reference's twelve gets it in `types`.

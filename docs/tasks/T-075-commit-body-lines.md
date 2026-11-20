# T-075: Prohibited body lines and a body full stop above the trailers

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | git plugin 0.3.0 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `5d7f1b6` (T-074 merged) |
| Touches | `plugins/git/src/commit_message.rs`, its tests, `plugins/git/rules/commit-message.md`, `plugins/git/CHANGELOG.md` |

## Goal

These are the follow-ups recorded in [T-074](T-074-gate-checks-itself.md).

1. **`prohibited-body-lines`.** `git/commit-message` gets a new option: a list of regular expressions,
   empty by default and read like `prohibited-trailers`.
   - Each pattern is searched in each line of the message's body.
   - The body lines are every line after the header that is not blank and is not in the trailer
     block. Comment lines, a trailing `Conflicts:` block and the scissors section are left out, as
     git does.
   - A line that one pattern matches is a finding of the new aspect `prohibited-body-lines`, and the
     finding names the line.
   - An ignored message, such as a merge or a `fixup!`, is checked too, as for trailers.
   - A pattern that does not compile is a configuration error. So is an empty pattern, which would
     refuse every line.

   So a `Generated with ...` body line is refused, and a `Generated-with:` trailer is left to
   `prohibited-trailers`.
2. **`body-full-stop` above the trailers.** `body-full-stop` judges the body without its trailer
   block. That is the block `git interpret-trailers` finds, the one `prohibited-trailers` reads.
   - A body that ends with a full stop and then has a `Co-authored-by: A Person` block passes.
   - A body made only of a trailer block has nothing to end with a full stop, so it passes.
   - `body-max-line-length` still reads the whole body.

## Review fixes

The independent review of `b50ce6b` found two defects, which are fixed here:

1. **Prose inside the trailer block.** Under git's quarter rule, a trailer block with a
   `Signed-off-by` line may also hold lines that are neither a trailer nor a continuation, and
   `body_lines` dropped them. `Generated with an assistant` above `Signed-off-by: A Person` in the
   last paragraph escaped `prohibited-body-lines`. These lines are now body lines, and so is an
   indented line after one of them. An indented line after a trailer continues that trailer; it
   is read, unfolded, by `prohibited-trailers`.
2. **An issue reference in the block.** A trailer block that holds an issue reference, such as
   `Refs: #12`, starts the parser's footer at that line, so the body ended inside the block and
   `body-full-stop` judged it whole. A body that ends with the first lines of the block, as a
   paragraph of its own, is now judged above them.

Also: the cut uses the body's real line offsets, CRLF included. The rule page notes that `^`
lets a line that starts with an emoji or a space through, and that the option applies to pushed
commits. The root CHANGELOG has a 0.3.0 Features entry.

Known git-parity behaviour, kept on purpose:

- A line that starts with `#` is a comment and is not read, also in a pushed commit's message,
  where git stored it (`git commit --cleanup=verbatim`). The message is read as `commitlint`
  reads it, with `#` as the comment character.
- A last paragraph of one `Token: value` line, such as `Note: see the guide`, is a trailer block,
  as git finds it. It is not a body line for `prohibited-body-lines`, and `body-full-stop` judges
  the body above it.

## Done when

- Tests, each with a pass and a fail:
  - a `Generated with` body line is refused and the same text in the trailer block or the header is
    not;
  - a line no pattern matches passes, and the option is empty by default;
  - patterns that cannot work fail closed;
  - a pushed commit's body line is reported on that commit;
  - for `body-full-stop`, a body with a full stop above a trailer block passes, a body without one
    still fails, and a trailer-only body passes.
- The rule page and the plugin's CHANGELOG are updated under 0.3.0.
- One lane round on T-075's own target directories, with hooks off in the round worktree. These
  must exit 0: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the release build. The log
  must show the git plugin compiled from T-075's checkout, with every new test listed. The leak scan
  prints nothing.

## Resume point

2026-11-20T18:36:16+00:00: the review fixes and the aspect count fix (`b50ce6b`) are on `7f62554b`.
Round 363 on `042c9f0` was red on the count alone; ticket 371 was stopped in the queue for the
review. Next: one lane round on the fixed tip.

2026-11-20T17:20:00+00:00: `c7f9539` has the option, the `body-full-stop` change, the tests, the rule
page and the CHANGELOG. Its lane round, ticket 361, was stopped in the queue before it started, at
the lead's request. T-073 goes first, because on the current base `deny` fails on
RUSTSEC-2026-0587. Next: once T-073 is on `main`, rebase onto it and run the lane round.

2026-11-20T17:11:47+00:00: branch `task/T-075-commit-body-lines` from `main` `5d7f1b6`; task file
written. Next: the option, the `body-full-stop` change and the tests.

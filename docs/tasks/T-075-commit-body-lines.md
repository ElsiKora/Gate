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

2026-11-20T17:11:47+00:00: branch `task/T-075-commit-body-lines` from `main` `5d7f1b6`; task file
written. Next: the option, the `body-full-stop` change and the tests.

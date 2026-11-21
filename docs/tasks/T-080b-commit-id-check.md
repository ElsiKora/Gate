# T-080b: Refuse an exception's commit id only where it is one

| Field | Value |
| --- | --- |
| Status | lane ticket 425 green on `3ea2349d`, every mutation caught; waits for the lead |
| Release | 0.3.0 (git plugin) |
| Writer | the T-068 writer |
| Reviewer | an independent session, then the lead |
| Depends on | `main` at `6d7081a1` (T-080 merged) |
| Touches | `plugins/git/src/history.rs`, `plugins/git/src/commit_message.rs`, `plugins/git/src/no_ci_skip.rs`, `plugins/git/tests/git/repo.rs`, `plugins/git/tests/git/commit_message.rs`, the pages of `git/commit-message` and `git/no-ci-skip`, `plugins/git/CHANGELOG.md`, `docs/guide/configuration.md` |

## The bug

T-080 made `excepted_commits` (`plugins/git/src/history.rs`) refuse every exception target of 7 to
64 hex digits that is not a full lower-case commit id. A `git/commit-message` exception is keyed by
a body line or a trailer too, and a body line can be hex digits. With
`prohibited-body-lines = ["^\\d+$"]`, the exception for the line `20261121` is a configuration
error, so it cannot be written at all.

## Change

A target is refused only where it really is a commit id:

- A full-length target (40 or 64 hex digits) with an upper-case digit is refused outright. The
  error gives the id in lower case.
- A shorter hex target (7 digits or more) is refused when `git` resolves it to a commit whose id
  begins with it (`rev-parse --verify <target>^{commit}`). The error gives the commit's full id.
- Any other hex target names no commit and is text: it passes, as a header, a body line or a
  trailer does.

`excepted_commits` takes the run's `Git` for that. `git/commit-message` and `git/no-ci-skip` pass
theirs. An exception keyed by a short id resolves once per run, only for hex targets.

The test repository (`plugins/git/tests/git/repo.rs`) resolves a revision as git does: a full id,
then a ref by its full or short name (`refs/`, `refs/tags/`, `refs/heads/`, `refs/remotes/`), then
an abbreviated id of four or more hex digits of any case that begin exactly one commit's id. Its
commit ids now show their order within the first 7 characters (`c{order:06x}{order:033x}`), so a
7-digit prefix names one commit.

## The review's fix round

The independent review of `79875e2e` found one blocking point and two for the docs.

1. **A ref named like the target was refused as a commit id.** `rev-parse` reads a ref before an
   abbreviated id, so in a repository that tags releases by date (tag `20261121`), the exception
   for the body line `20261121` was a configuration error with wrong advice. A branch named
   `deadbeef` was the same. The target is now refused only when the resolved commit's id begins
   with it in lower case. The test repository reads short ref names before abbreviated ids, as
   git does, and a new test covers a tag and a branch named like their targets.
2. **The docs now say what this file says.** The configuration guide and both rule pages say the
   verdict depends on the clone and that a prefix shared by two commits is text.
3. Two points stay as they are, under Known behaviour: a full-length upper-case hex body line, and
   the wording for an annotated tag.

## Known behaviour

- A short target that is ambiguous in the repository resolves to nothing, so it passes as text and
  matches no finding. Its exception is then reported as stale where the rule ran. That is the report
  the author needs.
- Whether a short target resolves can change as commits arrive. A target of eight digits that a new
  commit's id begins with becomes a configuration error. Git's 7-digit abbreviations make this rare.
- A short id of a commit a shallow clone lacks resolves to nothing there, so it is text in that
  clone and an error in a full one.
- Where a ref is named like a short id that also begins a commit's id, git reads the ref, so the
  target is text.
- A body line of 40 or 64 hex digits with an upper-case digit is refused as an upper-case commit
  id, so it cannot be excepted. Such a line is a hash in a message; this rule does not tell it from
  an id.
- The docs and the error speak of commits only. A short id of an annotated tag object peels to
  its commit, whose id does not begin with it, so that target is text.

## Tests

In `plugins/git/tests/git/commit_message.rs`:

- `an_exception_with_a_short_or_upper_case_commit_id_is_a_configuration_error`: the 7-digit and
  12-digit prefixes of a commit's id, and the 12-digit prefix in upper case, are refused with the
  full id. The full id in upper case is refused with its lower-case form. A full lower-case id and a
  header pass.
- `a_hex_exception_target_that_names_no_commit_is_text_and_passes`: under
  `prohibited-body-lines = ["^\\d+$"]`, the exceptions `20261121` and `deadbeef0` are no error, and
  the body line's finding is on the target `20261121`.
- `a_hex_exception_target_that_names_a_tag_or_branch_is_text_and_passes`: with a tag `20261121`
  and a branch `deadbeef` on commits whose ids do not begin with those names, both exceptions are
  no error, and the body line's finding is on the target `20261121`.

## Done when

- One lane round on the pushed tip, its own target directory and logs kept: fmt, clippy, test, deny
  and the release build exit 0. The log shows `gate-plugin-git` compiled from this task's round
  checkout and lists both tests by name.
- Each mutation of the check fails its test: the resolve check dropped, every short hex target
  refused, the upper-case full-length refusal dropped, the resolved id's prefix check dropped.
- An independent review, one fix round, then the lead decides.

## Proof

Lane ticket 425 on `3ea2349d`, 2026-11-21T06:27:29+00:00 to 2026-11-21T06:44:23+00:00
(`round.sh`, its own checkout, logs kept under the ticket): fmt 0, clippy 0, test 0 (1536 passed,
0 failed), deny 0 (advisories, bans, licenses, sources ok), release build 0, leak scan 0. The test
and release logs show `Compiling gate-plugin-git` from the round's own checkout, and the test log
lists the three tests by name, each `ok`.

Each mutation made its exact test fail (exit 101). Two of them ran in ticket 425; the other two
missed their text there, since the fix round split the resolve call over three lines, and ran in
ticket 430 on the same commit (2026-11-21T06:52:43+00:00 to 2026-11-21T06:52:48+00:00):

| Mutation | Ticket | Test |
| --- | --- | --- |
| the upper-case full-length refusal dropped | 425 | `an_exception_with_a_short_or_upper_case_commit_id_is_a_configuration_error` |
| the resolved id's prefix check dropped | 425 | `a_hex_exception_target_that_names_a_tag_or_branch_is_text_and_passes` |
| the resolve check dropped | 430 | `an_exception_with_a_short_or_upper_case_commit_id_is_a_configuration_error` |
| every short hex target refused | 430 | `a_hex_exception_target_that_names_no_commit_is_text_and_passes` |

## Resume point

2026-11-21T06:53:15+00:00: tickets 425 and 430 on `3ea2349d` are green with every mutation caught; the branch waits for
the lead.

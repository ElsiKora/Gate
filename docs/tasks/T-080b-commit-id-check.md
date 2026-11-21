# T-080b: Refuse an exception's commit id only where it is one

| Field | Value |
| --- | --- |
| Status | written on `main` at `6d7081a1`, its lane round next |
| Release | 0.3.0 (git plugin) |
| Writer | the T-068 writer |
| Reviewer | an independent session, then the lead |
| Depends on | `main` at `6d7081a1` (T-080 merged) |
| Touches | `plugins/git/src/history.rs`, `plugins/git/src/commit_message.rs`, `plugins/git/src/no_ci_skip.rs`, `plugins/git/tests/git/repo.rs`, `plugins/git/tests/git/commit_message.rs`, the pages of `git/commit-message` and `git/no-ci-skip`, `plugins/git/CHANGELOG.md`, `docs/guide/configuration.md` |

## The bug

T-080 made `excepted_commits` (`plugins/git/src/history.rs`) refuse every exception target of 7 to
64 hex digits that is not a full lower-case commit id. A `git/commit-message` exception is keyed by
a body line or a trailer too, and a body line can be hex digits. With
`prohibited-body-lines = ["^\\d+$"]`, the exception for the line `20261121` is a configuration error,
so it cannot be written at all.

## Change

A target is refused only where it really is a commit id:

- A full-length target (40 or 64 hex digits) with an upper-case digit is refused outright. The
  error gives the id in lower case.
- A shorter hex target (7 digits or more) is refused when `git` resolves it to a commit
  (`rev-parse --verify <target>^{commit}`). The error gives the commit's full id.
- Any other hex target names no commit and is text: it passes, as a header, a body line or a
  trailer does.

`excepted_commits` takes the run's `Git` for that. `git/commit-message` and `git/no-ci-skip` pass
theirs. An exception keyed by a short id resolves once per run, only for hex targets.

The test repository (`plugins/git/tests/git/repo.rs`) resolves an abbreviated id as git does: four
or more hex digits of any case that begin exactly one commit's id. Its commit ids now show their
order within the first 7 characters (`c{order:06x}{order:033x}`), so a 7-digit prefix names one
commit.

## Known behaviour

- A short target that is ambiguous in the repository resolves to nothing, so it passes as text and
  matches no finding. Its exception is then reported as stale where the rule ran. That is the report
  the author needs.
- Whether a short target resolves can change as commits arrive. A target of eight digits that a new
  commit's id begins with becomes a configuration error. Git's 7-digit abbreviations make this rare.

## Tests

In `plugins/git/tests/git/commit_message.rs`:

- `an_exception_with_a_short_or_upper_case_commit_id_is_a_configuration_error`: the 7-digit and
  12-digit prefixes of a commit's id, and the 12-digit prefix in upper case, are refused with the
  full id. The full id in upper case is refused with its lower-case form. A full lower-case id and a
  header pass.
- `a_hex_exception_target_that_names_no_commit_is_text_and_passes`: under
  `prohibited-body-lines = ["^\\d+$"]`, the exceptions `20261121` and `deadbeef0` are no error, and the
  body line's finding is on the target `20261121`.

## Done when

- One lane round on the pushed tip, its own target directory and logs kept: fmt, clippy, test, deny
  and the release build exit 0. The log shows `gate-plugin-git` compiled from this task's round
  checkout and lists both tests by name.
- Each mutation of the check fails its test: the resolve check dropped, every short hex target
  refused, the upper-case full-length refusal dropped.
- An independent review, one fix round, then the lead decides.

## Proof

None yet.

## Resume point

2026-11-21T04:28:03+00:00: the change, the tests and the docs are written on `main` at `6d7081a1`;
the lane round is next.

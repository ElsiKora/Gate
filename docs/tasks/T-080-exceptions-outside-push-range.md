# T-080: Exceptions are due only where their rule judged what they cover

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead |
| Release | 0.3.0 (core, SDK, plugin interface `gate:plugin@0.3.0`, git plugin) |
| Writer | the T-068 writer |
| Reviewer | an independent session, then the lead |
| Depends on | `main` at `de888663` |
| Touches | `wit/gate.wit`, `crates/gate-sdk`, `crates/gate-runtime`, `crates/gate-host/src/findings.rs`, `crates/gate-host/src/baseline.rs`, `crates/gate-host/src/plugins.rs`, the result cache, `plugins/git`, their tests and pages, `docs/guide/configuration.md`, `CHANGELOG.md` |

## The bug

`Findings::finish` (`crates/gate-host/src/findings.rs:331`) makes an exception a `gate/exceptions`
error when two things hold:

- its rule ran in the exception's scope;
- the exception matched no finding.

"Ran" means "the rule's call finished". It does not mean "the rule looked at what the exception
covers".

The page of `git/commit-message` tells users to accept a commit others already have with an
exception keyed by its commit id. The rule runs in every hook, but it judges that commit in only one
run: the push that brings it. Every other run fails on the exception:

- in pre-commit (`--staged`), the rule judges nothing;
- in commit-msg (`--commit-message`), it judges the new message;
- in a later push, it judges the new commits, not the one already on the remote.

So once the exception is committed, every commit and every push fails until it is removed. Removing
it brings the finding back in the run that judges the commit.

The same thing happens wherever a rule's subjects change from run to run:

- `git/branch-name` judges one branch per run, the pushed or checked-out one. An exception for one
  branch name fails every run on another branch.
- `git/no-ci-skip` judges the message being written only in commit-msg. An exception keyed by that
  header fails every other run.
- A rule may narrow its work to the change selection (`tree.changed()`, which the SDK allows). An
  exception for a file outside the selection would then fail too. No rule narrows today.

Exceptions keyed by a path, for a project the selection skips, already work: the rule did not run in
that scope, so its exceptions are not due.

The failing test is
`an_exception_for_a_pushed_commit_is_not_due_in_a_run_that_does_not_judge_it` in
`crates/gate/tests/plugins.rs`. It covers pre-commit, commit-msg, and a push without the commit. Two
guards pass today and must keep passing:

- `an_exception_for_a_commit_the_push_judges_and_accepts_is_reported`;
- `an_exception_for_a_commit_the_history_no_longer_holds_is_reported`.

## When an unused exception is stale

An exception says that one finding stays. It is stale when that finding no longer occurs. A run can
show this only if its rule judged what the exception covers and reported nothing for it. A run that
did not look shows nothing, either way.

So the rule is:

> An exception is due when its rule ran in its scope **and judged its target** in that run. A due
> exception that matches no finding is a `gate/exceptions` error.

Only the rule knows what it judged. The core cannot work it out:

- The push range is too wide. `git/commit-message` leaves out the commits main already holds, so a
  commit inside the range may be one it did not judge.
- A plugin call's read set covers a whole plugin call, not one rule. One `git` call holds
  `main-history`, which reads all of main.

So the rule must say what it judged.

The rule above alone would let one stale exception hide. A commit is judged once, when it is pushed.
Later, the commit could be rewritten, or the rule's options could change so that the commit now
passes. Either way, its exception accepts nothing, and no later run would judge the commit again.

To close that gap, a rule also gets the targets of its own exceptions. A rule whose subjects change
from run to run judges every excepted target it can judge on its own, in every run.

## Design

### Plugin interface (`gate:plugin@0.3.0`, not released yet; 0.3.0 already changes it)

- `check-request` gains `excepted: list<excepted-targets>`. An `excepted-targets` is
  `{ rule: string, targets: list<string> }`: for each rule in the request, the `target` of each of
  its exceptions in the call's scope, as written. These are the strings exceptions are matched with.
- `check` returns `check-result { findings: list<finding>, judged: list<judged-targets> }`. A
  `judged-targets` is `{ rule: string, targets: list<string> }`.
  - A rule listed in `judged` judged exactly those targets in this call.
  - A rule not listed judged its whole scope, as every rule does today.

### SDK

- `Run::excepted() -> &[String]` returns the targets of the running rule's exceptions.
- `Run::judges_only(targets)` narrows the rule's view to the targets listed. It may be called more
  than once; each call adds targets. A rule that calls it with no target judged nothing.
- A rule that never calls `judges_only` judged its whole scope.
- `gate_sdk::check` (the native test harness) returns the judged targets with the findings, so a
  plugin can test them.

### Core

- In `Findings::finish`, an exception is due when both of these hold:
  - its rule ran in its scope;
  - the rule did not narrow there, or it listed the exception's `target` (or its `path`) as judged.
- A narrowed rule's baseline entries follow the same test. An entry is due when its `key` or `path`
  is among the judged targets (`Baseline::stale`, and with it `gate baseline prune`).
- The result cache stores `judged` with the findings. `excepted` is part of the request, so it is
  already part of the call's key: a changed exception file runs the call again.
- `--rule` and `--fail-fast` keep their meaning. A rule that did not run has no due exceptions.

### Plugins

- **`git/commit-message`** narrows in every run. It judges:
  - each pushed commit it checks, by its id;
  - in commit-msg, the targets its findings on the message would carry: the header, each body line
    and each trailer;
  - each excepted target that is a full commit id, in every run:
    - When the id is an ancestor of the run's head, the rule reads that commit and checks its
      message as it checks a pushed one, with the current options.
    - When it is not, it was rewritten or dropped, so the rule judges it with no finding.
  - In a `--rev` run and a plain run, only the excepted commits.
- **`git/branch-name`** judges the branch it checks, or nothing on a detached HEAD, on a push to a
  tag, and on main outside a hook. It also judges each excepted name, with the run's hook flag. That
  verdict depends only on the name, the options and the flag.
- **`git/no-ci-skip`** judges its whole history, by id, plus the header of the message in commit-msg.
  An excepted header is judged only when the message carries it.
- Every other rule is unchanged: it judges its whole scope.

### What a run still does not report

A target a rule cannot judge on its own is due only in the runs that hold it. An example is an
excepted header text for a message no one writes again. The exception then stays unreported, but it
also accepts nothing in any run. A full run (`gate check`, no selection) judges every file rule's
whole scope, so a stale path exception is reported there.

### Rejected

- **The core decides from the run's inputs** (push range, change selection), with no word from the
  rule. It is wrong both ways:
  - After a merge of main into a branch, the range holds main's commits, which
    `git/commit-message` leaves out. So exceptions for them would fail.
  - It cannot see `git/branch-name`'s subject.
- **A commit-keyed exception is never due unless its commit left the history.** This hides an
  exception that the rule's new options no longer need.
- **A per-project option to turn the check off.** The lead ruled it out, and it would hide truly
  stale exceptions.

## Tests

- Host, end to end (`crates/gate/tests/plugins.rs`): the three tests above. A test for
  `git/branch-name`: an exception for another branch name is not due on this branch. And one for a
  name the policy now accepts: that exception is reported.
- `findings.rs`: the due test for:
  - a narrowed rule that listed the target;
  - a narrowed rule that did not list it;
  - a rule that did not narrow;
  - the path form of a target.
  The same cases for a baseline entry.
- The result cache: a stored `judged` answers the second run as it did the first, and a changed
  exception file runs the call again.
- `plugins/git`: the judged targets of each mode of the three rules, through `gate_sdk::check`.

## Docs

- `docs/guide/configuration.md`: when an exception is reported.
- The pages of the three rules: what each judges.
- The SDK page: `judges_only` and `excepted`.
- `CHANGELOG.md`: a Bug Fixes entry under 0.3.0.

## Resume point

Design written; the failing test is committed (`dee6440c`) and runs in the lane. Next: the lead's
answer on the design.

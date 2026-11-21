# T-080: Exceptions are due only where their rule judged what they cover

| Field | Value |
| --- | --- |
| Status | review fix round: written on `main` at `6b36eeac`, its lane round next |
| Release | 0.3.0 (core, SDK, plugin interface `gate:plugin@0.3.0`, git plugin) |
| Writer | the T-068 writer |
| Reviewer | an independent session, then the lead |
| Depends on | `main` at `6b36eeac` |
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

The lead approved the design in full, the excepted half included, on these conditions:

- build on the Gate 001 stack (`task/T-052-check-time-thresholds`), which changes the same check
  request;
- a rule that never calls `judges_only` keeps today's behaviour exactly;
- the judged list is part of the cached result, and the exceptions are part of the call's key;
- re-judging excepted commits reads only those commits;
- the docs and both CHANGELOGs are updated.

### Plugin interface (`gate:plugin@0.3.0`, not released yet; 0.3.0 already changes it)

- The request side was already on the stack: `rule-run.exceptions`, the targets of the rule's
  exceptions in the call's scope as written, which the SDK hands out as `Run::exceptions()`. Its
  `Debug` form is part of the result cache's key, so a changed exception list runs the call again.
- `check` returns `check-result { findings: list<finding>, judged: list<judged-targets> }`. A
  `judged-targets` is `{ rule: string, targets: list<string> }`.
  - A rule listed in `judged` judged exactly those targets in this call.
  - A rule not listed judged its whole scope, as every rule does today.
- The runtime refuses a `judged` entry for a rule the invocation did not run, as it refuses such a
  finding.

### SDK

- `Run::judges_only(targets)` narrows the rule's view to the targets listed. It may be called more
  than once; each call adds targets, and repeats are dropped. A call with no target means the run
  judged nothing.
- A rule that never calls `judges_only` judged its whole scope, and the SDK lists nothing for it: a
  plugin built without the new call answers an empty `judged`.
- `gate_sdk::check_output` answers the findings and the judged targets. `gate_sdk::check` still
  answers the findings alone, so no existing caller changes.

### Core

- `RulesRan` records the narrowed rules by scope (`narrow`). `RulesRan::judged(scope, rule, target)`
  holds when the rule ran there and either did not narrow or listed `target`.
- In `Findings::finish`, an exception is due when its rule judged the exception's `target` in its
  scope, or in the repository scope for the root's file. The design's "or its `path`" is dropped:
  the rule receives the targets as written and lists them as written, so one comparison covers
  both.
- A baseline entry is due when its rule judged the entry's `key`. An entry without a key is due
  whenever its rule ran, as before: the core cannot tell what such an entry is about.
  `Baseline::stale`, and with it `gate baseline prune`, use the same test.
- The result cache stores `judged` with the findings (format 5), and a cached call answers it as
  the call did.
- `--rule` drops a filtered rule's narrowing with its run. `--fail-fast` keeps its meaning.

### Plugins

- **`git/commit-message`** narrows in every run. It judges:
  - each pushed commit it checks, by its id;
  - in commit-msg, the targets its findings on the message carry: the header, each body line and
    each trailer;
  - in every run, each excepted target that is a full commit id (40 or 64 lower-case hex digits):
    - When the run's head (the range's head, else `HEAD`) holds it, the rule reads that commit
      alone (`git log <id>^!`) and checks its message as it checks a pushed one, with the current
      options.
    - When the head does not hold it, the commit was rewritten or dropped, so the rule judges it
      with no finding.
  - In a `--rev` run and a plain run, only the excepted commits.
- **`git/branch-name`** judges the branch it checks, if any. It finds none on a detached HEAD, on a
  push to a tag, or in a repository without commits. It also judges each excepted name in every run,
  by the strictest verdict: the one a hook gives, `prohibited` included, without "main passes
  outside a hook". This differs from the approved "with the run's hook flag". Under the run's flag,
  an exception for `main` would accept nothing in CI and be reported there, although every hook
  needs it. A name that passes the strict verdict passes in every run, so its exception is truly
  stale. A current branch that an exception names is judged the same way.
- **`git/no-ci-skip`** judges its whole history by id, each excepted commit id, and the header of
  the message in commit-msg. An excepted id outside the history (before activation, or rewritten)
  gets no finding, so its exception is reported.
- Every other rule is unchanged: it judges its whole scope.

### Cost

A run with N excepted commit ids of `git/commit-message`:

- asks N ancestry questions (`is-ancestor`, which the core answers from its store once known);
- runs at most N single-commit `git log <id>^!`;
- never walks the history.

`git/no-ci-skip` reads no more commits than before. `git/branch-name` adds one verdict per excepted
name, with no git call. The judged list adds N ids per narrowed call to the stored result;
`git/no-ci-skip` also adds its history's ids.

In a shallow clone, an excepted commit past the cut reads as not held, so its exception is reported
there. The CI job fetches the whole history for a rule that reads it, unless `fetch-depth` is set.

### The review's fix round

The independent review found nothing that blocks the design, and asked for one fix round:

1. **A re-judged commit brought new findings.** Re-judging an excepted commit reported every aspect
   of it. So an exception for a commit's `header-max-length`, plus a later `prohibited-trailers`
   pattern that the commit matches, made every pre-commit and commit-msg run fail on every branch
   that holds it, over a commit no one can change. The core now drops them, not the rule:
   - `judged-targets` gains `rejudged: list<string>`, the targets a rule judged only to re-check
     its own exceptions (`Run::rejudges`), outside what the call judges otherwise.
   - The core keeps a finding on a re-judged target only when an exception accepts it, so the
     aspects the exceptions do not cover are left to the run that judges the target in full.
   - Handing each rule the aspects of its exceptions was the other way. It would change
     `rule-run.exceptions` from a list of targets to records, and with it the fixtures of every
     plugin's tests. The core already holds the exceptions with their aspects.
   - `git/commit-message`, `git/branch-name` and `git/no-ci-skip` re-judge their excepted targets
     outside their own subjects. A target a rule both judges and re-judges is judged in full.
2. **A short or upper-case commit id was never due.** `excepted_commits` skipped any target that is
   not a full lower-case id. A target of 7 to 64 hex digits that is not one is now a configuration
   error. It names the exception and says to write the full lower-case id (`git rev-parse <id>`).
   A finding could not report it, because the exception itself would accept that finding: its
   target is the same text.
3. **Baseline keys did not line up.** A pushed commit's body-line and trailer findings are keyed
   `<id> <line>`, and the rule listed only `<id>`. It now lists both keys of each commit it judges
   in full, so such a baseline entry is due where the commit is judged in full (its push). A target
   only re-judged does not make a baseline entry due: a re-check drops what no exception accepts,
   so a baseline entry would read as stale there. `gate baseline write` runs a full run, which
   judges no commit, so it writes no such entry; one can come only from a push run's findings.
4. The split doc comment in `crates/gate-runtime/src/runtime.rs` and the long line in
   `crates/gate-host/src/baseline.rs`.

### Known behaviour

- **An excepted commit on another branch, or past a shallow clone's cut.** The head of a run on a
  branch that does not hold the commit cannot read it. So the commit is judged with no finding, and
  the exception is reported in that run. An exception keyed by a commit belongs on the branch that
  holds the commit, which is where it is committed. A shallow clone hides commits past its cut, so
  the CI job fetches the whole history for a rule that reads it, unless `fetch-depth` is set.
- **Path exceptions of a narrowing rule.** An exception is due when its target, as written, is among
  the targets the rule listed. A rule that narrows its view and reports findings with no target,
  matched by path, must list those paths as the exceptions write them. None of the three narrowing
  git rules reports such a finding.

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

- Host, end to end (`crates/gate/tests/plugins.rs`):
  - `an_exception_for_a_pushed_commit_is_not_due_in_a_run_that_does_not_judge_it`: pre-commit,
    commit-msg, and a push without the commit. It failed on the stack before the fix.
  - `an_exception_for_a_commit_the_push_judges_and_accepts_is_reported`: a guard.
  - `an_exception_for_a_commit_the_history_no_longer_holds_is_reported`: a guard.
  - `a_changed_exception_list_is_not_answered_from_the_result_cache`. Two `--staged` runs, with
    HEAD unchanged; the second is cached. Then a staged change of the exception list runs the call
    again, and reports the newly named good commit's exception.
- `crates/gate-host/tests/host/findings.rs`:
  - the due test of an exception: a rule that judged its whole scope, one that judged the target,
    one that judged nothing, another project's narrowing, and a matched exception;
  - the same test for baseline entries, with and without a key, in `check` and in `prune`.
  - Every existing exception and baseline test is unchanged and must stay green: a rule that never
    narrows behaves as before.
- `crates/gate-host/src/cache/results.rs`: a stored `judged` reads back as written.
- `crates/gate-sdk/tests/judged.rs`:
  - a rule that never calls `judges_only` is not listed;
  - one that calls it is listed with its targets, without repeats;
  - one that re-judges is listed with those targets apart, without the ones it judged in full;
  - one that judged nothing is listed empty.
- `crates/gate-host/tests/host/findings.rs`,
  `a_finding_on_a_target_re_judged_for_its_exceptions_stays_only_when_an_exception_accepts_it`:
  - on a re-judged commit, the excepted aspect is accepted, and another aspect is dropped;
  - the exception is due once the excepted aspect passes;
  - a baseline entry keyed by a re-judged target is not due;
  - judged in full, the other aspect is reported.
- `crates/gate/tests/plugins.rs`,
  `an_excepted_commit_re_judged_outside_its_push_brings_no_finding_its_exception_does_not_cover`.
  This is the review's input: an exception for the commit's `header-max-length`, then a
  `prohibited-trailers` pattern the commit matches. Pre-commit and commit-msg pass; the push that
  brings the commit reports the trailer.
- `plugins/git/tests/git/commit_message.rs`,
  `an_exception_with_a_short_or_upper_case_commit_id_is_a_configuration_error`, and the push test
  lists each pushed commit's `<id> <line>` keys.
- `plugins/git/tests/git`: what each mode judges, through `gate_sdk::check_output`:
  - `git/commit-message`: a plain run; `--rev`; an excepted commit held and good, held and bad, and
    not held; a push; and the message's header, body line and trailer;
  - `git/branch-name`: the current branch; excepted names, by the strict verdict; `main` outside a
    hook; a detached HEAD;
  - `git/no-ci-skip`: its history, an excepted id outside it, and the header in commit-msg.

## Docs

- `docs/guide/configuration.md` (Exceptions): when an exception is reported, and the same rule for
  the baseline.
- `docs/guide/writing-a-rule.md`: "What a run judged", which covers `judges_only`, `exceptions()`
  and `check_output`.
- `docs/architecture.md`: the interface's `check-result`.
- The pages of `git/commit-message`, `git/branch-name` and `git/no-ci-skip`: what a run judges.
  The commit-message page also gives the cost.
- `CHANGELOG.md` and `plugins/git/CHANGELOG.md`: Bug Fixes under 0.3.0.

## Resume point

Lane round 397 was green on `main` at `3ce3be63`. The review's fix round is written on `main` at
`6b36eeac`: re-judged targets, short commit ids, baseline keys, and the two style points. Next:
its lane round, then the report by log to the lead.

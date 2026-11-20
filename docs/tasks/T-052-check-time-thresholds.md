# T-052: Two check-time thresholds, a warning and a failure

| Field | Value |
| --- | --- |
| Status | in progress (design approved by the lead, 2026-11-19) |
| Release | v0.3.0 (it replaces two options of `gate check`; with T-048) |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | T-061's tip `d2f6710` (over T-059 and T-048, on `main` at `7f62554`); the four merge as one batch |
| Touches | `crates/gate-host/src/lib.rs` (the run options, `gate/budget`), `crates/gate` (the CLI), `crates/gate-host/src/config` (`[hooks]`, CI jobs), `crates/gate-host/src/sync` (hook and workflow generation, sync-state version), tests, docs, `CHANGELOG.md` |

## Goal

A run has one budget today: `--budget N` warns over N seconds, and `--budget-fails` turns that same
N into an error. A repository wants both at once, for example a warning over 3 s and a failure over
6 s, so a slowing gate is seen before it blocks anyone.

## Design

`gate check` takes two options and no other budget option:

- `--warn-after <seconds>`: a run that takes longer is a `gate/budget` warning;
- `--fail-after <seconds>`: a run that takes longer is a `gate/budget` error.

Either or both may be given; with both, `--fail-after` must be greater than `--warn-after`, else a
usage error (exit 2) that says so. A run over both reports one finding, the error, naming both
thresholds. `--budget` and `--budget-fails` are removed with no alias: an unknown option, as any.

Configuration has the same two keys, in one form, wherever a budget is set today:

```toml
[hooks]
budget = { warn-after = 3, fail-after = 6 }

[ci.github.jobs.gate]
run = ["check --push"]
budget = { warn-after = 120, fail-after = 300 }
```

Each key is optional, at least one is required, each is above 0, and `fail-after` > `warn-after`
when both are set: configuration errors with the file and key. The hooks and the generated jobs pass
exactly the thresholds they are given; nothing is added (today the jobs add `--budget-fails`, the
hooks do not). The integer form `budget = 300` is not kept: it fails as a value of the wrong type,
with the expected table in the message. `SYNC_STATE_VERSION` rises, since generated commands change.

## Tests

- `gate check`: over `--warn-after` only, a warning and exit 0; over `--fail-after`, one error and
  exit 1, which names both thresholds when the run is over both; under both, nothing.
- `--fail-after` equal to or below `--warn-after`, a threshold of 0, `--budget` and `--budget-fails`:
  exit 2.
- `[hooks] budget` and a job's `budget`: the integer form, an empty table, a 0, the order error and
  an unknown key are configuration errors; each key alone and both load.
- The generated hooks pass both thresholds to every check; a job passes exactly its thresholds, and
  an entry that writes its own threshold keeps exactly that, with nothing that makes it fail.
- A `check` entry of `[hooks]` or a job that `gate check` would refuse is a configuration error
  naming the entry: `--budget`, thresholds out of order, a 0, more seconds than a run can last,
  options that conflict. The entries are parsed with the definition `gate check` parses
  (`gate_host::cli`), so one definition serves both (review of `c552c19`).
- `--warn-after 1e20` exits 2, not a panic; a threshold written `--fail-after=200` counts as written.
- The round runs actionlint on a workflow generated with a `budget` table.

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the
  release build exit 0, actionlint on a generated workflow exits 0, and the leak scan prints
  nothing.
- Do not edit docs/plan.md.

## Resume point

2026-11-20T20:40:47+00:00: stack round 375 (ticket 000000000375) on this branch's tip `d52d7a0`, the
top of the stack T-048, T-059, T-061 on `main` at `4c09a04`, was green: fmt, clippy, test, deny,
release build, sync, actionlint and the leak scan all exit 0; all 18 changed crates compiled from
the round's own checkout, and the 4 tests this branch adds passed by name. Rebased onto T-061 on
`main` at `d0c6cda`: main moved the checks of a CI job into `check_ci_job`, which now refuses a
budget table through `check_budget`; main's sync state 7 (the workflow meets `ci/recommended`) is
taken, so this branch's change of hooks and workflows is state 8; the guide's job example keeps
main's `timeout-minutes` and writes the budget as a table; main's end-to-end test of the CI standard
wrote `budget = 300`, which this branch refuses, so it writes the table too; the plan keeps main's
rows and names the time thresholds where it named `--budget`. One lane round on this branch's tip
counts for all four; next: that round.

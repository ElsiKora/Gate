# T-052: Two check-time thresholds, a warning and a failure

| Field | Value |
| --- | --- |
| Status | in progress (design approved by the lead, 2026-11-19) |
| Release | v0.3.0 (it replaces two options of `gate check`; with T-048) |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | the T-048 tip `7c2b723`; both rebase when main moves and merge after the v0.2.1 tag |
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

- Over the warning only: a warning, exit 0. Over the failure: one error, exit 1. Under both:
  nothing.
- `--fail-after` not above `--warn-after`: exit 2. `--budget`: exit 2 as an unknown option.
- `[hooks] budget` and a job's `budget`: each key alone, both, 0, the order error, the integer form.
- Generated hooks and workflow pass exactly the configured thresholds; a sync of a 0.2.0 workflow is
  drift.

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the
  release build exit 0, actionlint on a generated workflow exits 0, and the leak scan prints
  nothing.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: design approved by the lead; branch `task/T-052-check-time-thresholds` from the T-048
tip `7c2b723`; next: the code.

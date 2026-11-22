# T-095: a generated CI job runs after other generated jobs

| Field | Value |
| --- | --- |
| Status | design sent to the lead |
| Release | `v0.3.1` (additive; the plugin interface stays `gate:plugin@0.3.0`) |
| Writer | "Gate 011" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `ec3b2c9` |
| Touches | `crates/gate-host/src/config` (`[ci.github.jobs.<id>]` keys and their checks), `crates/gate-host/src/sync/github.rs` (the workflow), tests in `crates/gate/tests` and `crates/gate-host/tests`, `docs/guide/configuration.md`, `CHANGELOG.md`, `docs/plan.md`, this task file |

## Goal

A consumer's reports job runs after the build job and every test suite, whenever they finished,
and the aggregate job needs it. `[ci.github]` in 0.3.0 has no job-level `needs` and no job-level
`if`, so a generated workflow cannot hold that job. A job of `[ci.github.jobs]` names the generated
jobs it runs after and whether it needs them to succeed or only to finish.

## Design

Two keys of `[ci.github.jobs.<id>]` (`after` is taken: the steps after the gate commands):

- `needs`: the ids of jobs of `[ci.github.jobs]` the job runs after, each once.
- `when`: `"succeeded"` (the job runs only when every job in `needs` succeeded) or `"completed"`
  (it runs whenever they finished and the run was not cancelled). Required with `needs`, refused
  without it: no default, so there is one way to write each mode.

```toml
[ci.github.jobs.reports]
name = "Reports"
needs = ["build", "unit", "e2e"]
when = "completed"
timeout-minutes = 10
run = ["run reports"]
```

The generated job writes GitHub `needs:` (`changes` first when the job lists `projects`, then the
named jobs in the order written) and one `if:`:

| `when` | without `projects` | with `projects` |
| --- | --- | --- |
| `completed` | `${{ !cancelled() }}` | the projects condition, which starts with `!cancelled()` |
| `succeeded` | `${{ !cancelled() && needs.build.result == 'success' && ... }}` | the projects condition `&& needs.build.result == 'success' && ...` |

The projects condition is the one 0.3.0 writes:
`!cancelled() && (needs.changes.result != 'success' || contains(fromJSON(needs.changes.outputs.projects), 'server'))`.
Never a bare `success()` (it would skip the job when `changes` failed), never `always()`, never an
expression from the configuration. A `succeeded` job whose named job was skipped (its projects did
not change) is skipped too; the aggregate counts a skipped job as passing. The aggregate job still
needs every job, this one included.

Configuration errors at load (exit 2), each after `[ci.github] jobs.<id>: `:

- `needs names "x", which is not a job of [ci.github.jobs]; name a job id the table has`
- `needs names the job itself; a job cannot run after itself`
- `needs names "changes", the generated job that lists the changed projects; list projects instead`
- `needs names "<aggregate>", the aggregate job, which runs after every job; remove it`
- `needs names "x" twice; name each job once`
- `when is not set; with needs, write when = "succeeded" (every job in needs succeeded) or when = "completed" (they finished and the run was not cancelled)`
- `when is set without needs; name the jobs in needs or remove when`
- `when = "x"`: refused by the parser, naming the two values
- a cycle: `needs forms a cycle: a -> b -> a; remove one of these needs`

## Questions answered from the code

- The aggregate job's check name: `aggregate_value` in `crates/gate-host/src/sync/github.rs` writes
  `name: <id>`, and GitHub names a job's check by its `name`, so the id is the check name branch
  protection requires. There is no separate name key for it.
- A matrix job: `[ci.github]` has no matrix. Each cell becomes a job of its own with its own `run`
  (`unit`, `e2e`, ... each `run = ["run <suite> --push"]`), and the job after them lists every cell
  in `needs`.

## Done when

- The keys, their checks and the generated `needs:`/`if:` exist as designed, with tests for each
  refusal, both modes, the combination with `projects`, the aggregate needing every job, and
  `gate sync --check` drift.
- The `ci` plugin's `ci/recommended` passes on a generated workflow that uses the keys.
- The guide's `[ci.github]` section and a `0.3.1` heading of `CHANGELOG.md` describe them; no
  version moves.
- Every command of `AGENTS.md` exits 0 through the lane; the leak scan prints nothing.

## Proof

Not yet run.

## Resume point

2026-11-22T20:25:31+00:00: task file written on `task/T-095-ci-job-dependencies` off `main` at
`ec3b2c9`; the design went to the lead. Next: wait for the lead's answer, then implement.

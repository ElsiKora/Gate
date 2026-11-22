# T-095: a generated CI job runs after other generated jobs

| Field | Value |
| --- | --- |
| Status | in review |
| Release | `v0.3.1` (additive; the plugin interface stays `gate:plugin@0.3.0`) |
| Writer | "Gate 011" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `1f5012d` |
| Touches | `crates/gate-host/src/config` (`[ci.github.jobs.<id>]` keys and their checks), `crates/gate-host/src/sync/github.rs` (the workflow), tests in `crates/gate/tests/cli.rs` and `crates/gate/tests/plugins.rs`, `Cargo.lock` (`url` 2.5.9), `docs/guide/configuration.md`, `CHANGELOG.md`, `docs/plan.md`, this task file |

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

The lead approved the design as written, and asked the guide and the tests to state how a skipped or
failed job in `needs` behaves, and the guide to say one job per cell in place of a matrix.

`cargo deny check` failed on `main`'s lock (`1f5012d`): RUSTSEC-2026-0592, `url` 2.5.8. A commit of its
own, `fix(deps): url 2.5.9`, runs `cargo update --offline -p url --precise 2.5.9` (`url` 2.5.9,
`form_urlencoded` 1.2.3, nothing else).

Lane round on `8122c6c`, `one-gate.sh` in Gate's lane, from 2026-11-22T20:46:01+00:00 to
2026-11-22T21:06:22+00:00 (load 50 to 180), in a detached worktree of the commit, each workspace
crate compiled from it:

| Command | Exit |
| --- | --- |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --offline --locked --workspace --all-targets --keep-going -j 4 -- -D warnings` | 0 |
| `cargo test --offline --locked --workspace -j 4 --no-fail-fast` | 0 (1606 passed, 0 failed) |
| `cargo deny --offline --locked check` | 0 (advisories, bans, licenses, sources ok) |
| `cargo build --release --offline --locked -j 4` | 0 |

The tests of this task in that run, each `ok`: `a_completed_job_runs_after_its_jobs_whatever_their_result_unless_the_run_is_cancelled`,
`a_succeeded_job_runs_only_when_every_job_it_names_succeeded`,
`a_job_with_needs_and_projects_needs_changes_first_and_keeps_the_projects_condition`,
`the_aggregate_job_needs_a_job_with_needs_too`, `sync_check_reports_an_edited_needs_or_if_of_a_job_with_needs`,
`a_needs_or_when_that_cannot_hold_is_a_configuration_error`, and `the_generated_workflow_meets_the_ci_standard`
with a `needs`/`when` job under `ci/recommended`.

The leak scan of the local rules printed nothing, for the tree and with `--branch origin/main`.

## Resume point

2026-11-22T21:06:25+00:00: on `task/T-095-ci-job-dependencies`, rebased on `main` at `1f5012d`; every
gate green on `8122c6c`. Next: the lead reads the change and names a reviewer; no merge here.

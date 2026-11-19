# T-017: An aggregate CI job, and `[generate]` checked at load

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | none (from main `3aa6b74`) |
| Touches | `crates/gate-host/src/config/**`, `crates/gate-host/src/sync/github.rs`, the plugin load path in `crates/gate-host/src/plugins.rs`, their tests, docs/architecture.md §7 and §10 |

## Goal

1. `[ci.github]` takes an optional `aggregate = "<job id>"`. The generated workflow then has one more
   job with that id that `needs` every other generated job, runs `if: ${{ always() }}`, and fails
   unless every needed job's result is `success` or `skipped`. A repository makes that one job its
   required check, whatever jobs the configuration adds later.
2. A `[generate]` key that names no generator of a declared plugin is a configuration error in the
   commands that load the plugins, `gate check`, `gate sync` and `gate sync --check`, raised before
   anything runs. The message names the key and the generators the plugin declares. `gate list` stays
   configuration-only: it loads and fetches no plugin, so it does not see generators (lead, 2026-11-18).

## Why

The gate/sync acceptance cases on main `f73e09c` (notes `gate003-gate-sync-investigation.md`):

- The first consumer's CI had an aggregate job as its single required check. Main's `[ci.github]`
  generator has no such property, so the generated workflow has none, and branch protection that
  requires the aggregate would never be satisfied.
- A configuration naming generators the plugin does not have passes `gate check` and fails
  only in `gate sync --check`, as an environment error with exit 2. Architecture §7 says every section
  is validated at load.

## Scope

1. `aggregate` in `[ci.github]`: a job id; equal to another generated job's id, or not a valid job id,
   is a configuration error. Without it the workflow is unchanged byte for byte.
2. The aggregate job's step reads `toJSON(needs)` and exits 1 naming each needed job whose result is
   neither `success` nor `skipped`. It checks out nothing and installs nothing.
3. `[generate]` validation as in Goal 2. It uses the generator list the loaded components already
   declare; no new interface.
4. Architecture: one line in §7's `[ci.github]` description or example, and §10's "core's own" bullet
   names the aggregate.
5. Tests:
   - the generated workflow with `aggregate` (golden), and without it unchanged;
   - `gate sync --check` reports `gate/sync` when the aggregate job's `needs` or `if` is edited;
   - an aggregate id that collides with a job id is a configuration error;
   - `gate check`, `gate sync` and `gate sync --check` on a configuration naming an unknown generator
     exit 2 with the configuration error and write nothing, and the same configuration with a known
     generator passes.

## Done when

- The tests above, each failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0, run on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-18: planned; from main `3aa6b74` on `task/T-017-ci-aggregate`.
2026-11-18: written from main `5ed242b`. `[ci.github] aggregate` (config check against `changes` and the
job ids; the job in `sync/github.rs`, `jq` over `toJSON(needs)`); `[generate]` keys checked against the
loaded plugins' generators in `Loaded::validate`, so `gate check`, `gate sync` and `gate sync --check`
fail with a configuration error before anything runs. `gate list` stays configuration-only (lead,
2026-11-18). Tests in `crates/gate/tests/cli.rs` (aggregate) and
`crates/gate/tests/plugins.rs` (generators). Next: the gates on the pushed commit, and the
new tests run against main to show each fails there.
2026-11-18: review round (verdict "merge after fixes"): job ids of `[ci.github.jobs]` and `aggregate`
are a `JobId` that starts with a lowercase letter, since GitHub refuses a job id starting with a digit;
the aggregate command is tested under `bash -e` with `jq` (success and skipped pass; failure and
cancelled fail naming the job; outputs holding `$(...)`, quotes or a newline change nothing; skipped
with a message where bash or jq is missing); the `runs-on` doc and architecture §10 say the aggregate
runs there and needs bash and jq.

Merged to main after `a8a7322` (the merge tree equals it): fmt, clippy (workspace), test (830 passed),
deny and release build all exit 0.

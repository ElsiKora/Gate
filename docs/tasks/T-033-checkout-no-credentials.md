# T-033: The generated checkout keeps no credentials

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.2.0 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | the v0.1.0 root |
| Touches | `crates/gate-host/src/sync/github.rs` (the checkout step), `crates/gate-host/src/plugins.rs` (`SYNC_STATE_VERSION`), `docs/architecture.md`, the workflow tests |

## Goal

Every `actions/checkout` step of the workflow `gate sync` writes sets `persist-credentials: false`,
with `fetch-depth` beside it when the job sets one. A checkout that keeps the job's token in the
clone's git configuration hands it to every later step and to any artifact of the workspace;
zizmor's `artipacked` audit flags each of the five generated checkouts of a converted workflow.

There is no option to keep the credentials: the generated jobs never push, and they read private
plugin releases through `GATE_GITHUB_TOKEN`, not through git.

## Scope

1. `checkout` in `crates/gate-host/src/sync/github.rs` writes `with: { persist-credentials: false }`,
   and `fetch-depth` first when one is set. Its doc comment says why there is no option.
2. `docs/architecture.md`, where the CI generation is described, says the same.
3. `SYNC_STATE_VERSION` rises when a stored `sync-state` answer depends on the workflow text the core
   writes.

## Tests

- The generated workflow's checkouts carry `persist-credentials: false`, with and without
  `fetch-depth`.
- `gate sync --check` reports drift on a workflow generated before the change.

## Done when

- The tests above, failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: branch `task/T-033-checkout-no-credentials` on main `59fb163`, the first public release.
Change, docs and tests written. `SYNC_STATE_VERSION` is 5: a stored `sync-state` answer covers the
workflow the core writes (`sync::core_outputs`), and its key holds the core's version, which stays
0.1.0 on main until the v0.2.0 bump. Next: the gates.

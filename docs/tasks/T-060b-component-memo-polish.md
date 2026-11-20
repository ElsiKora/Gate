# T-060b: Polish the test component memo

| Field | Value |
| --- | --- |
| Status | written; lane round queued |
| Release | v0.2.1 (tests only) |
| Writer | "Gate 003: host foundations" |
| Reviewer | the lead |
| Depends on | `main` at `08668a8` (T-060 merged) |
| Touches | `crates/gate-runtime/tests/support/component.rs`, the doc lines of `crates/gate-runtime/tests/runtime.rs` and `crates/gate/tests/plugins.rs` |

## Goal

The three low findings of T-060's review, in one commit:

1. A build-log line is formatted first and written with one `write_all`, so two concurrent builds
   cannot interleave their lines. With no features the line has no trailing space.
2. A target directory is `<package>` without features and `<package>+<features>` with them. A
   package name has no `+`, so `foo` with `-bar` and `foo-bar` no longer share a directory.
3. The docs say what is true: once per process, which `cargo test` makes once per binary; a runner
   that starts one process per test (nextest) builds once per test, in parallel.

## Done when

- One lane round on the pushed tip: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, and the `gate-runtime` and `gate` test targets exit 0; the leak
  scan prints nothing.

## Proof

Not run yet.

## Resume point

2026-11-20T00:10:59+00:00: written from the lead's review of T-060; queued after T-065's round.

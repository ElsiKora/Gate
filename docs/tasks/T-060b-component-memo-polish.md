# T-060b: Polish the test component memo

| Field | Value |
| --- | --- |
| Status | proven; ready for the lead |
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

Lane ticket 271 on `01cf77d`, 2026-11-20T00:50:01+00:00 to 2026-11-20T00:56:44+00:00, `one-gate`
exit 0:

| Step | Command | Exit |
| --- | --- | --- |
| fmt | `cargo fmt --all --check` | 0 |
| clippy | `cargo clippy --offline --locked --workspace --all-targets -j 4 -- -D warnings` | 0 |
| test | `cargo test --offline --locked -p gate-runtime -p gate -j 4 --no-fail-fast` | 0, 174 passed, 0 failed |

`sort component-builds.log | uniq -c | cat -e`: 11 lines, each count 1, and each line ends right
after its package or its features (`gate-plugin-git$`, `gate-runtime-probe describe-traps$`), so no
line carries a trailing space. The run's target directories with features are
`gate-runtime-probe+describe-spins` and `gate-runtime-probe+describe-traps`.

## Resume point

2026-11-20T00:56:59+00:00: round 271 green as above; ready for the lead.

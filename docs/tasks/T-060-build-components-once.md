# T-060: Build each test component once per test binary

| Field | Value |
| --- | --- |
| Status | done (merged at `08668a8`) |
| Release | v0.2.1 (tests only) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `ec66737` |
| Touches | `crates/gate-runtime/tests/support/component.rs` (new), `crates/gate/tests/plugins.rs`, `crates/gate/tests/distribution.rs`, `crates/gate-runtime/tests/runtime.rs`, `crates/gate-runtime/tests/measure.rs` |

## Goal

A test that runs a plugin builds it with `cargo build` into `CARGO_TARGET_TMPDIR/<package><features>`
and hands the path to `gate check --plugin`. In `crates/gate/tests/plugins.rs` only some components
were kept in a `OnceLock` (hello, typescript, the probe); the git and agents plugins were kept in a
`OnceLock` per test function, and `component`/`component_with` built on every call. Tests run in
parallel, so a second build into the same target could replace the `.wasm` while another test's
run read it: lane round 253 of T-058 failed once on
`a_message_past_the_read_set_limit_is_recorded_by_its_digest` with "cannot read the file: No such
file or directory" for `gate-plugin-git/wasm32-wasip2/release/git.wasm`.

## Design

One helper, `crates/gate-runtime/tests/support/component.rs`, included with `#[path]` by every test
binary that builds a component (`gate`'s `plugins` and `distribution`, `gate-runtime`'s `runtime` and
`measure`). It keeps one `OnceLock` per (package, features) in a static map, so the first call
builds and every later call, from any test, gets the same path and bytes; a concurrent caller waits
on the cell. Each build appends `<test binary> <package> <features>` to
`CARGO_TARGET_TMPDIR/component-builds.log`. No product code changes.

Checked for the same pattern: `crates/gate-host/tests`, `crates/gate-sdk/tests`, `plugins/*/tests`
and `languages/*/tests` build no component (no `cargo build` in them).

## Done when

- Two full `cargo test --locked --workspace` runs in one lane round are green.
- The build log of each run lists each (binary, package, features) once.
- `cargo fmt`, `clippy -D warnings`, `cargo deny` pass in the same round; the leak scan prints
  nothing.

## Proof

Lane ticket 260 on `f6fafe4` (`round-inner.sh`: fmt, clippy, deny, then two full test runs, each
followed by `sort component-builds.log | uniq -c`), 2026-11-19T23:46:24+00:00 to
2026-11-20T00:06:20+00:00, `one-gate` exit 0:

| Step | Command | Exit |
| --- | --- | --- |
| fmt | `cargo fmt --all --check` | 0 |
| clippy | `cargo clippy --offline --locked --workspace --all-targets -j 4 -- -D warnings` | 0 |
| deny | `cargo deny --offline --locked check` (advisories, bans, licenses, sources ok) | 0 |
| test run 1 | `cargo test --offline --locked --workspace -j 4 --no-fail-fast` | 0, 1036 passed, 0 failed |
| test run 2 | the same, again | 0, 1036 passed, 0 failed |

The build log of each run had 11 lines and 11 distinct lines, so each (binary, package, features)
was built once:

| Binary | Package | Features |
| --- | --- | --- |
| `distribution` | `gate-plugin-git` | |
| `plugins` | `gate-plugin-agents` | |
| `plugins` | `gate-plugin-git` | |
| `plugins` | `gate-plugin-hello` | |
| `plugins` | `gate-plugin-ledger` | |
| `plugins` | `gate-plugin-nestjs` | |
| `plugins` | `gate-plugin-typescript` | |
| `plugins` | `gate-runtime-probe` | |
| `plugins` | `gate-runtime-probe` | `describe-traps` |
| `runtime` | `gate-runtime-probe` | |
| `runtime` | `gate-runtime-probe` | `describe-spins` |

`measure` builds nothing in this run: its one component-building test is `#[ignore]`d and runs only
when asked.

## Resume point

2026-11-20T00:06:42+00:00: round 260 green as above; ready for review. Next: the reviewer's round.

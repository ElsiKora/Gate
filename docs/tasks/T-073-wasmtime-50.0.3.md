# T-073: wasmtime 50.0.3, for RUSTSEC-2026-0587

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 |
| Writer | "Gate 002" |
| Reviewer | the lead (pins, lockfile and changelog only) |
| Depends on | `main` at `cee6491` |
| Touches | `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md` |

## Goal

`cargo deny check` fails on `main` since the advisory RUSTSEC-2026-0587 (a WASI filesystem sandbox
escape on Windows through a drive-prefixed path component) was published against `wasmtime-wasi`
50.0.2, which `gate-runtime` uses. 50.0.3 fixes it. The released binaries, `aarch64-apple-darwin`
and `x86_64-unknown-linux-musl` (`.github/workflows/reproducible.yml`), were not affected, since the
advisory is Windows-only.

## Scope

1. `Cargo.toml`: the pins of `wasmtime` and `wasmtime-wasi`, the only wasmtime-family pins, from
   `=50.0.2` to `=50.0.3`, in lockstep.
2. `Cargo.lock` by `cargo update -p wasmtime -p wasmtime-wasi --precise 50.0.3`, with the crates of
   the wasmtime release that follow it, and nothing else.
3. `CHANGELOG.md`: one Bug Fixes entry under 0.3.0 that names the advisory.
4. No other change.

## Done when

- `Cargo.lock` holds no 50.0.2 crate of the wasmtime family, and no crate outside that release
  changes version.
- One lane round on the pushed tip, on this task's own target directory: fmt, clippy, test, deny
  (advisories ok) and the release build exit 0, and its log shows the changed crates compiled from
  this task's worktree.

## Resume point

2026-11-20T17:02:09+00:00: the pins of `wasmtime` and `wasmtime-wasi` say `=50.0.3`; `cargo update
-p wasmtime -p wasmtime-wasi --precise 50.0.3`, run as a lane job, moved the 30 crates of that
wasmtime release (17 at 50.0.3, 13 `cranelift-*` at 0.137.3) and nothing else; `Cargo.lock` holds no
50.0.2 crate. The CHANGELOG has the Bug Fixes entry. Next: one lane round on T-073's own target.

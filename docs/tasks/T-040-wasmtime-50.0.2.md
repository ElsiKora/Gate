# T-040: wasmtime 50.0.2, for eight security advisories

| Field | Value |
| --- | --- |
| Status | merged |
| Release | v0.2.0 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | the lead |
| Depends on | `release/v0.2.0` at `1583d85`; merges as soon as it is green, ahead of the others |
| Touches | `Cargo.toml` (the two wasmtime pins), `Cargo.lock` (the wasmtime family only), `CHANGELOG.md`, and any use of a wasmtime API that 50.0.2 changes |

## Goal

`cargo deny --locked check` exits 0 again. The lane round of T-034 (round 183, 2026-11-19) failed
deny only, on eight RustSec advisories against the pinned `wasmtime` and `wasmtime-wasi` `=50.0.1`;
each names `>=50.0.2` as the fix for the 50 line:

- RUSTSEC-2026-0566: wasmtime-wasi: unbounded host memory growth via empty writes to
  `AsyncWriteStream`.
- RUSTSEC-2026-0567: abort in Wasmtime builds without the `threads` cargo feature.
- RUSTSEC-2026-0568: stale GC heap base pointer in compiled code lets a Wasm guest abort the host
  process.
- RUSTSEC-2026-0569: panic in `Caller::get_module_export` when a host function isn't called from
  WebAssembly.
- RUSTSEC-2026-0570: abort in Wasmtime builds without the `gc` cargo feature.
- RUSTSEC-2026-0571: wasmtime-wasi-http: empty body writes can result in unbounded heap usage with
  HTTP/2.
- RUSTSEC-2026-0572: panic when a component cancels a stream or future copy on the wrong end.
- RUSTSEC-2026-0573: process abort when a GC heap's reservation isn't a multiple of the page size.

## Scope

1. Both pins in the root `Cargo.toml` move to `=50.0.2`.
2. `cargo update -p wasmtime -p wasmtime-wasi`, run in the lane, moves only the wasmtime family in
   `Cargo.lock`; the commit message lists every crate whose version changed.
3. `CHANGELOG.md`, Unreleased, Bug Fixes: a security entry naming the eight advisories.
4. If 50.0.2 changes an API the runtime uses, the use is fixed here.

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the
  release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md.

## Resume point

2026-11-19T17:53:44+00:00: merged into `main` at `b3b2fe3` (branch tip `5912bb1`) and released in
`v0.2.0`. Nothing is left.

Earlier:

2026-11-19: branch `task/T-040-wasmtime-50.0.2` from `release/v0.2.0` at `1583d85`; task file
written; next: the pins, the lock update and one round in the lane, then the report.

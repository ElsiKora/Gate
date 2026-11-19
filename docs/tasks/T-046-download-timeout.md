# T-046: A download that stops sending fails within a bound

| Field | Value |
| --- | --- |
| Status | open |
| Release | the lead sets it |
| Writer | "Gate 005" |
| Reviewer | a separate review session |
| Depends on | v0.2.0 |
| Touches | `crates/gate-host/src/fetch/mod.rs` (the agent, `transport`, `stream`), `crates/gate-host/tests/host/http.rs` (a route that stops sending), the fetch and distribution tests, `crates/gate/src/launcher.sh` (the curl and wget calls), `docs/architecture.md` section 12, `CHANGELOG.md` |

## Goal

Every network read of the core and of `bin/gate` has a connect timeout and a read timeout. A server
that stops sending fails the download within that bound, with an environment error (exit 2) that
names the URL and says to try again. Nothing is retried in a loop, in Rust or in shell: one attempt,
one bound, one clear error; the user reruns the command.

Today the core's agent (`Fetcher::new`) sets `timeout_connect` to 30 s and `timeout_global` to 15
minutes and nothing per read, so a connection that stops sending holds `gate lock`, `gate sync` or a
plugin fetch silently for up to 15 minutes per request. The launcher's three `curl` calls set no
timeout at all, and its `wget` fallback keeps wget's defaults: a 900 s read timeout and 20 tries.

## Reproduction (2026-11-19)

- The released v0.2.0 core (`gate-aarch64-apple-darwin.tar.gz`, sha256 `9c824aab…`) on macOS arm64,
  `gate lock` in a scratch tree of three projects whose `gate.toml` files name core `0.2.0` and the
  seven plugins `ElsiKora/Gate:<name>@0.2.0`; an empty `GATE_CACHE_DIR`; no token; run under an outer
  `timeout 600`.
- In those 600 s the lock stored 4 of the 9 archives it pins in the cache's `blobs/sha256` (the two
  core archives, `agents.wasm`, `ci.wasm`), then wrote nothing more and printed nothing until the outer
  timeout killed it (exit 124). Standard output and standard error were empty: the lock prints only
  when it finishes, so it gave no sign of which request it was waiting on.
- Not the rate limit: right after, GitHub's API reported 54 of the 60 unauthenticated calls of the
  hour left. Not the release: minutes before, `curl` had fetched the same 16 release files (28 MB),
  every one matching its API digest, in seconds.
- With the five missing plugin archives put into the cache by their sha256, the same `gate lock`
  finished in 14.5 s, the eight release lookups only, and wrote a `gate.lock` that pins exactly those
  digests.
- Not known: whether the stalled request was the fifth archive (`git.wasm`, next in order) or a
  release lookup, and whether it stalled in the headers or in the body. Without the outer timeout it
  would have ended at the 15-minute global timeout of that one request.

## Scope

1. The core's agent (`Fetcher::new`) adds ureq 3.5.2's own per-read timeout,
   `timeout_per_read(Some(READ_TIMEOUT))`, 30 s: it restarts on every transport read and covers the
   TLS handshake, the response headers and the body, so it is the one mechanism for a stall in any
   phase. `timeout_resolve` (30 s) joins `timeout_connect` (30 s), which does not cover local name
   resolution. `timeout_global` stays as the outer bound on a download that trickles. The bounds are
   constants of the fetch module; the fetcher carries them so a test can shorten them. No environment
   variable, no retry.
2. One message for a timeout, wherever it surfaces: a `ureq::Error::Timeout` from `call()` (in
   `transport`) and an `io::Error` of kind `TimedOut` from the body reader (in `stream`) both read
   `cannot download <what> from <url>: no data for <n> s; try again` (the phase in the message when
   ureq names one). Other transport errors keep today's messages. A failed download leaves nothing in
   the cache and removes its temporary file, as today.
3. The launcher: every `curl` call (`anonymous`, the release lookup, the private asset) gets
   `--connect-timeout 30 --speed-limit 1 --speed-time 30`, curl's own stall bound; `wget` gets
   `--timeout=30 --tries=1`. The failure message names the URL and says to try again. No loop.
4. `docs/architecture.md` section 12 says that every download has a 30 s connect and read bound and
   fails rather than waits; `CHANGELOG.md` gets the entry.

## Tests

- `crates/gate-host/tests/host/http.rs` gains a route that answers `200` with a `Content-Length`, sends
  part of the body and then holds the connection open without sending, and one that accepts and never
  answers (`Route::delayed` past the bound). Against each, a fetch with a 1 s read bound fails within
  a measured 10 s with an environment error that names the URL and says `try again`, and the cache
  holds no blob and no temporary file afterwards.
- A route delayed less than the bound still downloads and verifies (the bound does not cut a slow
  but live server).
- `crates/gate/tests/distribution.rs`: the launcher against a core route that stops sending fails
  with exit 2 and the URL in its message, and installs nothing. curl's bound is 30 s, so this test
  takes about 30 s; it is the only slow one.
- All the existing fetch, lock and distribution tests unchanged.

## Done when

- The tests above, the new ones failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: branch `task/T-046-download-timeout` from main `89c42c7`. Task file written from the
reproduction above; no code yet. Next: the lead's word on scope (item 3, the launcher, is in because
`bin/gate` downloads the core the same way), then the change and the tests.

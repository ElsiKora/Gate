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

## Fetch paths

Every network read of a Gate run goes through one of two clients:

| Path | Client | Connect bound today | Read bound today |
| --- | --- | --- | --- |
| The core: `gate lock` (release lookups through `Fetcher::release`, then each pinned archive through `download_checked`), plugin fetches in `gate sync` and `gate check` (`Fetcher::fetch`), `[tools]` (`tools::install`), `gate init` | the one `ureq::Agent` built in `Fetcher::new` | 30 s (`timeout_connect`) | none; only the 15-minute `timeout_global` of the request |
| `bin/gate`: the core archive (`anonymous`, every redirect hop), a private release's JSON and asset | `curl`, else `wget` | curl's built-in 300 s; wget's 900 s | none for curl; wget's 900 s, with 20 tries |

The core starts no other network client: its `git` commands read the local repository only, and
`gh auth token` reads the local login. A second report fits the table: `gate lock` started through
`bin/gate` failed with curl's exit 28 and exit 2, a connect timeout in the launcher's core download,
before the core ran. The connect step has a bound on both paths; the read step has none that is
short.

## Scope

1. The core's agent (`Fetcher::new`) adds ureq 3.5.2's own per-read timeout,
   `timeout_per_read(Some(READ_TIMEOUT))`, 30 s: it restarts on every transport read and covers the
   TLS handshake, the response headers and the body, so it is the one mechanism for a stall in any
   phase. `timeout_resolve` (30 s) joins `timeout_connect` (30 s), which does not cover local name
   resolution. The 15-minute global bound stays as the outer bound on a download that trickles, as
   one budget for the whole download: a deadline set once, each redirect hop and the private
   asset's lookup given what is left of it (review, 2026-11-19). The bounds are
   constants of the fetch module; the fetcher carries them so a test can shorten them. No environment
   variable, no retry.
2. One message for a timeout, wherever it surfaces: a `ureq::Error::Timeout` from `call()` (in
   `transport`) and an `io::Error` of kind `TimedOut` from the body reader (in `stream`) both read
   `cannot download <what> from <url>: no data for <n> s; try again`; a resolve or connect timeout
   reads `no connection within <n> s; check the network (and HTTPS_PROXY, if one is needed) and try
   again`, as the launcher says (review, 2026-11-19). Other transport errors keep today's messages. A failed download leaves nothing in
   the cache and removes its temporary file, as today.
3. The launcher: every `curl` call (`anonymous`, the release lookup, the private asset) gets
   `--connect-timeout 30 --speed-limit 1 --speed-time 30`, curl's own stall bound, and `--max-time`
   with what is left of one 15-minute deadline for the download; `wget` gets `--timeout=30
   --tries=1`, and has no overall limit, which section 12 says (review, 2026-11-19). The failure message names the URL and says to try again. No loop.
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
- A listener that accepts the TCP connection and never speaks TLS, with a 1 s connect bound, fails
  within 10 s with exactly the connect message (review, 2026-11-19).
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

2026-11-19: 1e96a8e reviewed; the four review points fixed: the connect message keeps the proxy
hint; one 15-minute deadline per download in the core (`within`) and in the launcher (`--max-time`
from `$deadline`); wget's missing overall limit in section 12; the connect-phase test. Next: one
lane round on the fix commit, its exit codes in a Proof section.

2026-11-19: round ticket 235 on 5e412d6: fmt 0, clippy 101 (`clippy::too_many_lines`, `download_into` 105/100;
the deadline and the bounds made its three `stream` calls longer); test, deny and build skipped by the
lane rule. `download_into` now streams every 200 answer through one closure. Read against the lints
that stopped other rounds: the timeout closure's `bound` beside `bounds` (`similar_names`) and the test
server's two-arm `match` on the stall (`single_match_else`) changed. Next: the round again.

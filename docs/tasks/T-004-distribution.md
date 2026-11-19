# T-004: Distribution

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 004: public presentation" |
| Reviewer | a separate review session |
| Depends on | T-002 |
| Touches | `crates/gate-host/src/{lock,fetch,cache}/**` (new modules), `crates/gate/src/**` (the `init`, `lock`, `cache` commands and `--plugin`/`--no-overrides`), `bin/gate` template, `.github/workflows/release.yml`, their tests |

## Goal

One way to install the tool and one way to install a plugin: a pinned release verified by sha256,
fetched once into a shared cache, with no compiler on a consumer's machine
([architecture.md](../architecture.md) sections 9, 12 and 16).

## Scope

1. **Lock**: `gate.lock` (TOML, committed) holds, for the core, the URL and sha256 of
   `gate-<target>.tar.gz` for each supported target, and for every plugin the resolved URL and sha256
   of `<plugin>.wasm`. `gate lock` resolves `<owner>/<repo>:<plugin>@<version>` to the asset of release
   tag `plugin-<plugin>-v<version>` and the core's `[gate] version` to release `v<version>`.
   `gate lock --set <name>=<url> --sha256 <hash>` pins one plugin without a download (the product
   release commit). A run whose lock does not match the configuration fails and says to run `gate lock`.
2. **Verified fetch**: download only URLs written in the lock, over HTTPS, into a temporary file in the
   cache, verify sha256, then rename into place. A mismatch fails closed (exit 2), names the asset, the
   URL and both hashes, and deletes nothing it did not write. Credentials for private assets, in order:
   `$GATE_GITHUB_TOKEN`, `$GITHUB_TOKEN`, `gh auth token`; a missing or refused one fails closed (exit 2)
   with the asset, the URL and what to set. A credential never appears in output, logs or errors.
3. **Cache**: `$GATE_CACHE_DIR` (default the user cache directory), content-addressed by sha256, shared
   by worktrees, atomic rename, safe under concurrent first use (two processes fetching the same asset
   both succeed), no network once filled. Layout leaves room for T-003's compiled modules, keyed by
   (sha256, wasmtime version, target triple), and the result and git caches. `gate cache prune` removes
   entries no lock in the given repositories references (dry run by default, `--yes` to delete).
4. **Override**: `--plugin <name>=<path.wasm>` replaces one pinned plugin for this run only, is never
   cached as that plugin, is printed in the report, is refused by `gate lock` and by `--no-overrides`.
5. **Launcher**: `gate init` writes `bin/gate` (POSIX `sh`, executable, committed) and a starting
   `gate.toml` when there is none. The launcher reads the core's version, URL and sha256 for the
   current target from `gate.lock`, fetches and verifies once into the cache (curl or wget, `shasum` or
   `sha256sum`), and `exec`s it with the arguments. It fails closed the same way as the core.
6. **Release workflow**: `.github/workflows/release.yml` on tag `v*` builds the two targets
   (`x86_64-unknown-linux-musl`, `aarch64-apple-darwin`) with `--locked`, packs
   `gate-<target>.tar.gz`, writes `SHA256SUMS`, and creates the release; actions pinned by commit.
   A second workflow on tag `plugin-*-v*` builds that plugin for `wasm32-wasip2` and uploads
   `<plugin>.wasm` and its sha256. No secret beyond the default `GITHUB_TOKEN`.

Not in scope: running plugins (T-003), generated hooks and CI (T-005), publishing a release (the lead).

## Done when

- fmt, clippy, test, deny exit 0.
- Tests on a local HTTP fixture server (no network): fetch and verify; checksum mismatch fails closed
  and leaves the cache as it was; missing credential for a private asset fails with exit 2 and the
  token never appears in output; two concurrent first fetches both succeed with one cache entry; a lock
  that does not match the configuration fails; `--plugin` refused by `gate lock` and `--no-overrides`;
  `lock --set` writes exactly the pin; `cache prune` keeps referenced entries.
- The launcher runs under `sh` (dash-compatible) in a test against the fixture server.
- The release workflow passes `actionlint` or an equivalent offline check.
- The leak scan named in the local rules prints nothing.

## Notes

- HTTP client: `ureq` 3.5.2, `rustls` on `ring` with bundled `webpki-roots`: blocking (no async
  runtime), no OpenSSL, no duplicate crate versions; the bundled roots let the static musl binary run
  without a system certificate store. `deny.toml` allows ISC, BSD-3-Clause and CDLA-Permissive-2.0
  for that stack.
- Downloads are `https` only; plain `http` is accepted for `127.0.0.1`, `localhost` and `[::1]` with a
  numeric port, so the tests can use a local fixture server. A URL that names a user (`@`) is refused.
  Neither the core nor `bin/gate` lets the HTTP client follow a redirect: each hop is checked like the
  first URL. wget (in `bin/gate`, when there is no curl) gets `https` only.
- A cache hit is hashed again and must be a regular file; anything else is removed and downloaded
  again. `bin/gate` records the sha256 of the unpacked core next to it (`core/<sha256>/gate.sha256`)
  and checks the binary against it before every `exec`. A download over 1 GiB is refused; a
  published digest is checked before the download enters the cache.
- `CACHEDIR.TAG` is written the first time a cache is filled. `gate cache prune` refuses a directory
  without it, and refuses a symbolic link among the layout's directories; a linked entry is removed
  as the link.
- `gate.lock` keys plugins by source, so two projects may pin two versions of one plugin; `--set`
  needs every declaration of the name to agree on one source.
- GitHub: github.com, and optionally one GitHub Enterprise named by both `GATE_GITHUB_URL` and
  `GATE_GITHUB_API_URL` (one without the other is an error). `ElsiKora/Gate`, the core and the shared
  plugins, always resolves through github.com; other repositories through the Enterprise when one is
  named. A GitHub on this machine (the test fixture) stands in for github.com.
- Credentials: a download is anonymous first; a refused release asset URL is retried through that
  GitHub's API. `$GATE_GITHUB_TOKEN` and `$GITHUB_TOKEN` belong to the Enterprise when one is named,
  else to github.com; `gh auth token --hostname <host>` is asked for either. The credential goes only
  to `<api>/repos/<owner>/<repo>/releases/`, the asset URL must be that repository's
  `releases/assets/<id>`, and the redirect to storage is followed without it.
- `bin/gate` fetches the core the same way, so a private core release works. The token reaches curl in
  a 0600 header file (`-H @file`), never on a command line. With no credential, or a refused one, it
  exits 2 naming the asset, the repository and what to set. A private core needs curl.
- Every command but `gate init` and `gate lock` refuses to run when it is not the core
  `[gate] version` asks for (exit 2, naming `bin/gate`).
- A change to `gate.lock` selects every project: the workspace reads the lock while the snapshot
  records inputs, so the lock is among the inputs every project depends on.
- `modules/` in the cache belongs to wasmtime's own cache (the plugin runtime's `Config::cache`).
  Its file names do not name a plugin, so `gate cache prune` keeps it whole while any lock pins a
  plugin and removes it whole when none does.
- A configuration with no plugins runs without a lock; with plugins, the lock must exist, be tracked
  and match.

## Resume point

2026-11-16T21:45:18+00:00: second fix round (the review of 864f716) done on top of 4fea8f3; in review. MAJOR 1 and
MINOR 2 to 7, nits 10, 11 and 13, and optional 9 and 12 are in; MINOR 8's report-shows-the-override
case waits for the plugin runtime (a check with a plugin cannot reach the report on this branch; the
summary line is tested in `findings.rs`). Optional 14 (`init --force`, one target) is not done.
Commands, all exit 0: `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D
warnings`, `cargo test --locked` (143 passed), `cargo deny --locked check`, `actionlint` (1.7.12),
`shellcheck -s sh crates/gate/src/launcher.sh`, `dash -n crates/gate/src/launcher.sh`, and the leak
scan. Open for later tasks: `gate cache prune --older-than | --max-size` (section 16) is not built,
prune is by lock references; the changelog in release notes.

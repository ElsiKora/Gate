# T-053: Pin a local build under its future URL, and check exactly that file

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 (breaking: `--sha256` is removed) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `v0.2.0` |
| Touches | `crates/gate/src/main.rs`, `crates/gate/src/distribution.rs`, `crates/gate-host/src/lock/`, `crates/gate-host/src/plugins.rs`, `crates/gate/tests/distribution.rs`, `README.md`, `docs/architecture.md`, `docs/guide/` |

## Goal

A plugin's release job pins the build it is about to publish, and `gate check` runs exactly that
file before the asset exists. Today `gate lock --set <name>=<url> --sha256 <hash>` writes a pin to
an asset that does not exist yet; `gate check` then runs only if a blob with that sha256 happens to
be in the cache, and nothing supported puts it there.

## Design

`gate lock --set <name>=<url> --file <path>` replaces `--sha256`. It reads the file, computes its
sha256, admits the bytes into the blob cache under that sha256 (the same content-addressed entry a
verified download makes), and writes the pin `{ url, sha256 }` to `gate.lock`. The hash always comes
from the bytes, so no hand-typed checksum can disagree with the file. `gate check` then finds the
blob through the normal path (`Fetcher::fetch`: a cache entry that still hashes to the pin, else a
download verified against it) and runs that file. Nothing changes in how a pin is read: the entry
is an ordinary pin, and once the asset is published, a machine without the blob downloads it and
fails closed on a checksum mismatch if the published file is not the one pinned. When the blob is
missing and the asset is not found (HTTP 404 from a URL outside GitHub, or a GitHub release
without that asset), the error names the plugin and says what to do: publish the asset, or admit
the built file on this machine with `gate lock --set <name>=<url> --file <path>`. A refused or
missing credential keeps its own message: the asset may be private, and a CI without a token must
not be told to pin a local build. `--sha256` goes, with no alias: the
release job has the file it pins, and a consumer pinning someone else's published asset uses
`gate lock`, which downloads and hashes it. The order is checks first, then the cache, then the
lock: an undeclared name, a URL gate does not fetch or an unreadable file fails before anything is
admitted or written.

`check --plugin <name>=<file>` stays what it is: an override for one run that runs a build in
place of the pin, says so in the run's output, and is refused by `gate lock` and by the generated
CI jobs (`--no-overrides`). It is not compared with the pin, since its purpose is to run a build
that differs; the supported way to check the file that will be published is `--file`.

## Scope

1. `gate lock --set <name>=<url> --file <path>`; `--set` requires `--file`; `--sha256` removed.
2. The bytes enter the cache through the same temporary-then-rename insert as a download.
3. The plugin load error for a missing blob whose asset is not found says to publish or to admit
   the build with `--file`; a refused or missing credential gets no such advice.
4. README, `docs/architecture.md` (the release flow), `docs/guide/writing-a-plugin.md` and
   `docs/guide/configuration.md` name `--file`; the core CHANGELOG lists the removal of `--sha256`
   under `# 0.3.0 (unreleased)` / `### Breaking Changes`.

Not in scope: comparing a `--plugin` override with the pin; validating at `gate lock` time that the
file is a component of the named plugin (`gate check` does that when it loads it).

## Done when

- Tests, each failing with its change taken out:
  - the file admitted by `--set ... --file` is the one `gate check` runs, with the URL unreachable;
  - a different file under the pinned sha256 in the cache is refused: the check fails closed and
    runs nothing;
  - a pin whose blob is missing and whose URL cannot be downloaded fails closed with a message that
    names `--file`;
  - a download refused with HTTP 401 or 403, and a GitHub asset refused without a credential, carry
    no `--file` advice;
  - `--set` without `--file`, and `--sha256`, are errors.
- `cargo fmt`, `clippy -D warnings`, `cargo test`, `cargo deny` pass in one lane round.
- The leak scan prints nothing.

## Resume point

2026-11-19T20:58:27+00:00: the review of 2c05ee4 found the trust model sound and asked for one round: the
admit advice only when the asset is not found (a 401, a 403 or a missing credential gets none,
with a test), the CHANGELOG entry under `# 0.3.0 (unreleased)` / `### Breaking Changes`, and the
`bin/gate check` step in the plugin guide's release steps. Committed; not compiled yet. Round 248
on 2c05ee4 is queued; its successor on this commit records the proof here when green.

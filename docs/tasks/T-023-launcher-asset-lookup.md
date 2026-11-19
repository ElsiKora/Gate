# T-023: The launcher finds a private asset in pretty-printed release JSON

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.1.0 |
| Writer | the writer of T-022 |
| Reviewer | a separate review session |
| Depends on | main `86b7b04` (0.1.0-rc.1) |
| Touches | `crates/gate/src/launcher.sh` (the asset lookup only), `crates/gate/tests/distribution.rs` |

## Goal

`bin/gate` installs a core that is a private release asset, whatever layout GitHub gives the
release JSON.

## Cause

The rc.1 launcher, run on macOS against the private rc.1 release with a `gh auth token`
credential, failed with exit 2: "release v0.1.0-rc.1 of ElsiKora/Gate has no asset
gate-aarch64-apple-darwin.tar.gz", although the release has that asset. The launcher reads the
release with curl, and the GitHub API pretty-prints its JSON for curl's user agent, one key per
line; `gh api` gets it compact. The lookup turned each brace into a line break and read one
record per line, so in the pretty form an asset's `"url"` and `"name"` fell on different lines
and no record held both. Measured on the rc.1 release, read once each way: the lookup, run under
`/usr/bin/awk` (version 20200816), printed the asset's API URL for the compact form and nothing
for the pretty form. `awk` was not the cause.

## Scope

1. The lookup deletes the line breaks (`\r`, `\n`) of the release JSON before it turns braces into
   record ends. A JSON string holds no raw line break, so no value changes.
2. Nothing else in the launcher changes.

## Tests

- `the_launcher_finds_a_private_asset_in_compact_and_pretty_release_json`: the fixture serves the
  release as the GitHub API writes it (its keys in GitHub's order, an `upload_url` with braces, an
  author and per-asset uploader whose `url` is not an asset's, another target's asset first, and a
  body that names the asset inside a string), once compact and once pretty-printed. `bin/gate`,
  run under `dash` or `sh` with the system's `awk`, installs the core from both and requests only
  asset 7. On main's launcher the pretty case fails (the test run records it).
- The private-core test now gets the GitHub-shaped release (compact) as well.

## Done when

- The new test fails on main's launcher and passes on this one; `cargo fmt --all --check`,
  `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`,
  `cargo deny --locked check`, `shellcheck`, `actionlint` and the leak scan exit 0
  on the pushed commit.

## Resume point

2026-11-18: branch `task/T-023-launcher-asset-lookup` from main `86b7b04`. Fix and test written;
next: the gates on the pushed commit, then the report.

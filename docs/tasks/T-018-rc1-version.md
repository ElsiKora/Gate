# T-018: Version 0.1.0-rc.1

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 003: host foundations" |
| Reviewer | the lead reads the diff (a version change only) |
| Depends on | T-016, T-017 (merged) |
| Touches | `Cargo.toml`, `plugins/*/Cargo.toml`, `Cargo.lock`, tests that spell the core or a plugin version |

## Goal

The release workflows accept a tag only when it equals the version in `Cargo.toml`: `v<version>` for
the core, `plugin-<name>-v<version>` for a plugin. The workspace and every plugin are at `0.1.0`, so the
tag `v0.1.0-rc.1` would fail its first job. Set the workspace version and each plugin's version to
`0.1.0-rc.1`, so the core and the seven plugins can be tagged and released as rc.1.

## Scope

1. `version = "0.1.0-rc.1"` in the workspace and in each plugin that declares its own version;
   `Cargo.lock` updated with `cargo update --workspace --offline`.
2. Tests and fixtures that spell the running core's or a plugin's version take it from
   `env!("CARGO_PKG_VERSION")` or the crate's constant, not a literal, so a later version change touches
   only the manifests. Fixtures that describe a configuration of some other version keep their literal.
3. Documentation examples keep `0.1.0`: they describe the release, not the candidate.

## Done when

- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0, run on the pushed commit.
- `cargo metadata --locked --no-deps` gives `0.1.0-rc.1` for `gate` and for each `gate-plugin-*`.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-18: planned; from main on `task/T-018-rc1-version`.
2026-11-18: the workspace and the six plugins with their own version at `0.1.0-rc.1`; `Cargo.lock` by
`cargo update --workspace --offline` (only the 16 workspace members change). At rc.1 the tests of
`crates/gate` failed 49 times on fixtures that pin the running core (`[gate] version`, the lock's
`[core]`, the core's release tag, messages naming the running version); those take
`env!("CARGO_PKG_VERSION")` now. Plugin sources of synthetic plugins (`@0.1.0`), the host tests'
configurations, which no core version check reads, and the documentation examples keep `0.1.0`.

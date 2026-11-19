# T-016: Reproducible components

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | the writer of T-007a |
| Reviewer | a separate review session |
| Depends on | T-004 (merged) |
| Touches | `scripts/build-component.sh`, `scripts/pack-core.sh`, `.github/workflows/release.yml`, `.github/workflows/release-plugin.yml`, `.github/workflows/reproducible.yml`, `docs/architecture.md` section 12 |

## Goal

A released component, the core or a plugin, has a sha256 that depends only on the source, `Cargo.lock`
and the toolchain of `rust-toolchain.toml`. Anyone can rebuild it in another checkout path, with
another cargo home, and get the pinned sha256.

## Why

Two builds of `gate-plugin-nestjs` from one commit gave two sha256 values, so no pin could be checked
by rebuilding. Measured at `f73e09c` with cargo 1.99.1, every build run alone:

- **Feature unification.** One cargo invocation that builds several plugins unifies the features of
  the dependencies they share: with the seven plugins together, `nestjs.wasm` gets the `regex` crate's
  `perf` engines and `aho-corasick` (6,632,688 bytes) where alone it has neither (6,267,984 bytes).
  `resolver = "3"` separates features by target and dependency kind only, and cargo 1.99.1 ignores
  `resolver.feature-unification` without `-Z`. A workspace-wide build likewise adds features to 17 of
  the core's dependencies.
- **Paths.** Panic locations of dependencies hold `$CARGO_HOME/registry/src/...`; the core also holds
  the target directory (the build-script output of `cranelift-codegen`). `nestjs.wasm` built alone
  gave `71158738...` in two checkouts and `b085f8c2...` with another cargo home.
- **The Apple link.** `ring` compiles its C with `-gfull` on Darwin (`ring` 0.17.14 `build.rs`
  577-578). ld64 hashes the output, with the debug map of those objects (absolute source and object
  paths), into `LC_UUID` before strip. With every path remapped, two cores still differed in
  `LC_UUID` only. `ld -reproducible` and `-oso_prefix` did not help; `-Wl,-S` (no debug information in
  the output) did.
- **The archive.** `tar -czf` stores the file's mtime and owner, and gzip its own timestamp.
- Profile `trim-paths` is not stable in cargo 1.99.1.

With the three remaps (checkout, target directory, cargo home) `nestjs.wasm` gave `3ac56045...` in two
checkouts with two cargo homes; with `-Wl,-S` added, the core gave `3f8d098e...` in both.

## Scope

1. `scripts/build-component.sh <package> <target>`: one package per cargo invocation, with
   `--remap-path-prefix` for the checkout, the target directory (as `cargo metadata` resolves it) and
   the cargo home, each resolved to its physical path and passed to cargo in that spelling, and
   `-Wl,-S` for Apple targets. The flags go in `CARGO_ENCODED_RUSTFLAGS`; `RUSTFLAGS`,
   `CARGO_PROFILE_*` and `CARGO_INCREMENTAL` are removed. It is the one way to build a released
   component.
2. `scripts/pack-core.sh <binary> <archive>`: `gate-<target>.tar.gz` with a fixed mtime, owner 0 and
   no gzip timestamp, with GNU tar and bsdtar. The tar is written to a file before gzip, and the
   script fails and leaves no archive unless `gate` is the one member.
3. `release.yml` and `release-plugin.yml` build through the script; `release.yml` packs through
   `pack-core.sh`. No `.cargo/config.toml`: config `rustflags` cannot name the cargo home or the
   target directory, and `RUSTFLAGS` replaces it.
4. `reproducible.yml` on every push to `main` and `task/**`, and as the build of both release
   workflows (`workflow_call`, input `components`: `all`, `core` or `plugin:<directory>`): the selected
   components are built twice, in two checkout paths with two cargo homes, the core is packed twice,
   and any sha256 difference fails the job. Each release runner builds the core of its target; the
   plugins of `plugins/*` with a `cdylib` target are built on `ubuntu-24.04` only, the host they are
   released from (lead, 2026-11-18, after run 44307223391: every plugin module differs between the
   Linux and the macOS host while each host reproduces itself). The release jobs publish the first
   build's files only after the comparison passes, and compile nothing.
5. Architecture section 12 states the rule. The development commands of `AGENTS.md` stay.
6. Out of scope (lead, 2026-11-18): acceptance and local job scripts stay as they are; only release
   artifacts are pinned.

## Done when

- Local: `build-component.sh` in two worktrees with two cargo homes (distinct
  directories) gives equal sha256 for `gate` (`aarch64-apple-darwin`) and `nestjs.wasm`, and
  `pack-core.sh` twice gives equal archives.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test
  --locked`, `cargo deny --locked check` exit 0 on the pushed commit; `shellcheck` and `actionlint`
  are clean.
- `reproducible.yml` passes on the pushed branch, from `gh run list` and `gh run view`: the core on
  both runners, `x86_64-unknown-linux-musl` included, and every plugin on `ubuntu-24.04`.

## Resume point

2026-11-18: branch `task/T-016-reproducible-components` from main `3aa6b74`. First version `8e389a7`
reviewed: merge after eight fixes (pack-core fails closed; the release workflows build through
`reproducible.yml`; every plugin checked; physical paths; `CARGO_ENCODED_RUSTFLAGS`; resolved target
directory; no `CARGO_PROFILE_*`; runner image in section 12), written in `a86c6b3`. Its CI run
44307223391: each runner reproduced all its components, every plugin differed across the runners;
the lead chose plugins on `ubuntu-24.04` only. Rebased on main `aa6d78c` as `f373186`: CI run
44315806863 passed (core on both runners, every plugin on `ubuntu-24.04`); the local proof gave equal
`gate`, archive and `nestjs.wasm` in two worktrees with two cargo homes; the gates on `f373186`
exited 0 for fmt, clippy, test (830 passed), deny, shellcheck, actionlint and the leak scan. Waiting
for the lead.

Merged to main after `4a2143e` (tree f373186 plus the task file): fmt, clippy, test (830 passed), deny,
shellcheck, actionlint and leak scan exit 0; two-path proof exit 0; CI run
44315806863 success. Follow-up: docs/guide/writing-a-plugin.md names scripts/build-component.sh (v0.1.0).

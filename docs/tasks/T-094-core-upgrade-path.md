# T-094: How a repository with its own plugin adopts a core release that changes the plugin interface

| Field | Value |
| --- | --- |
| Status | in review |
| Release | after `v0.3.0` (documents only; no version moves) |
| Writer | "Gate 010" |
| Reviewer | the lead reads the change |
| Depends on | `main` at `ec3b2c9` |
| Touches | `docs/guide/writing-a-plugin.md`, `docs/guide/configuration.md`, `plugins/git/rules/main-history.md`, `docs/plan.md`, this task file |

## Goal

A consumer pins a plugin it builds itself and moves that pin only in a release commit
(`git/main-history`'s `release-plugin`). It asked how it adopts a core release that changes the
plugin interface, which loads only plugins built for it. `release-all-pins` was considered and
dropped: the branch cannot prove the rebuilt plugin before the merge, the option admits the wide
form in every release commit, and a tag does not say how a release job would learn the versions.
The accepted path is an upgrade branch with a pre-release build of the plugin; the guide states it.
The same consumer found that the hooks section says `--commit-message` checks the tree, which is
not so since 0.3.0.

## Scope

1. `docs/guide/writing-a-plugin.md`: a section "Adopting a Core Release That Changes the Plugin
   Interface" under "6. Release": one upgrade branch moves the core, the shared plugins and the
   plugin's own pin to a published pre-release build (`gate lock --set ... --file ...`); the
   branch's generated job (`--no-overrides`, fetching each pinned `url` with `GATE_GITHUB_TOKEN`)
   proves the set; `git/main-history` judges the merge by its form only; the next release commit
   moves the pin from the pre-release to the release under `release-plugin`.
2. `docs/guide/configuration.md`: a link to that section from `gate.lock`; the hooks text says
   `--commit-message` checks only the message.
3. `plugins/git/rules/main-history.md`: an interface change links to that guide section instead of
   recommending `release-all-pins`, which the page now describes only by what it admits, so the
   guide and the rule page agree (the lead's review).
4. `docs/plan.md`: rows for T-094 and for T-096, the backlog task that removes `release-all-pins`
   (T-095 is taken by another open branch).

No CHANGELOG entry: `CHANGELOG.md` has no Unreleased section, and earlier documents-only tasks
made none.

## Claims checked against the code

- A version is `<major>.<minor>.<patch>[-<pre-release>]` in a source and a lock key:
  `crates/gate-host/src/config/names.rs` (`is_version`, the `PluginSource` schema).
- `<file>#version` reads only the version the plugin's pin moves to: `plugins/git/src/pins.rs`
  (`judge` filters `moved != version`; `config` and `lock` record only the new version).
- A merge is judged by `body`, `branch`, `sync` and `tree`: `plugins/git/rules/main-history.md`.
- The generated job's `check` gets `--no-overrides`: `crates/gate-host/src/sync/github.rs`
  (`command_words`); every `bin/gate` step gets `GATE_GITHUB_TOKEN`, the secret or else the job's
  token: `crates/gate-host/src/sync/github.rs:56-67` (`TOKEN`, `gate_env`).
- `gate lock --set` needs an existing lock and pins only the named plugin's declared source:
  `crates/gate-host/src/lock/mod.rs` (`set`). A private asset is read by its tag through
  `/repos/<owner>/<repo>/releases/tags/<tag>`: `crates/gate-host/src/fetch/mod.rs`. That GitHub
  serves a published pre-release there and not a draft is GitHub's API behaviour, not Gate's code.
- A fetched asset with another sha256 fails closed with a checksum mismatch, and a missing asset
  with an environment error: `crates/gate-host/src/fetch/mod.rs`, `crates/gate-host/src/plugins.rs`.
- `git/main-history` loads the plugins a past commit pins: at a direct commit that changes a path
  outside `documentation`, and at its parent for a removed path, it asks `sync_state`
  (`plugins/git/src/main_history.rs`), which loads that commit's plugins
  (`crates/gate-host/src/plugins.rs`, `commit_plugins`); so the pre-release asset stays published.
- The example release workflow marks a tag whose version has a pre-release part `--prerelease`, so
  the pinned file is the one its release job built and published.

## Done when

- `bin/gate check` (the repository's hooks, after `bin/gate sync`) exits 0.
- The leak scan named in the local rules prints nothing.
- The lead has read the change.

## Resume point

2026-11-22T20:33:51+00:00: written on `task/T-094-core-upgrade-path` off `main` at `ec3b2c9`;
first head `4d07eb1`, then `b37c468` for the rule page. The review's fix round (keep the
pre-release published, publish it from the release job as `--prerelease`, pin the published file,
the configuration wording, publish before the push, the toolchain and `gate-test`, the sources
above) is the third commit. Next: the lead merges.

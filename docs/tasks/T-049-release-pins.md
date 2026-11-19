# T-049: a release of git/main-history that moves a plugin's pin

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead |
| Release | v0.2.1 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | `main` at `105ca02` |
| Touches | `plugins/git/src/main_history.rs`, `plugins/git/Cargo.toml`, `Cargo.lock`, its tests, `plugins/git/rules/main-history.md`, `plugins/git/CHANGELOG.md` |

## Goal

A repository that releases a Gate plugin of its own pins that plugin in its own `gate.toml` and
`gate.lock`, and its release commit moves the pin to the new version. Today `release-files` admits a
file whole, and only `package.json` and `package-lock.json` are limited to their version fields, so a
release commit that also changes another plugin's pin, or the core's, passes. With this task the
release commit may change the pin of that one plugin and nothing else in Gate's pin files; the core
pin may move with it only when the repository says so.

## Design

Two options of `git/main-history`, beside `release-files`:

- `release-plugin` (string, unset by default): the source of the plugin a release moves, without
  its version, `<owner>/<repo>:<plugin>`. Set, a release commit may change:
  - each `gate.toml` (the root's or a project's) only in the `source` of an entry that names that
    plugin, from `<owner>/<repo>:<plugin>@<old>` to `@<version>`, where `<version>` is capture 1 of
    `release-subject`, as `package.json`'s `version` must be;
  - the root `gate.lock` only in that plugin's entries: `[plugins."<owner>/<repo>:<plugin>@<old>"]`
    gives way to `[plugins."<owner>/<repo>:<plugin>@<version>"]` with its `url` and `sha256`.
  Any other change to either file (another plugin's `source` or entry, `[gate] version`, `[core]`,
  options, rules) is a `release` finding keyed `<file>#beyond-pin`; a pin moved to a version other
  than the release's is `<file>#version`; a file that does not read as TOML on one side is
  `<file>#unreadable`.
- `release-core` (boolean, `false` by default): the same commit may also move the core, the root
  `gate.toml`'s `[gate] version` and `gate.lock`'s `[core]` table, as a plugin interface change
  forces. It needs `release-plugin`; set alone it is a configuration error.

`gate.toml` and `gate.lock` are admitted by `release-plugin`, not by `release-files`: naming either
in `release-files` while `release-plugin` is set is a configuration error, since the file would be
admitted whole and limited at once. Without `release-plugin`, nothing changes. The files are
compared as parsed TOML, as `package.json` is compared as JSON, so a change of layout or comments
alone is not seen. They are read at the parent and at the commit through the plugin interface's
git reads (`read_at`), like `package.json`. Merge and direct commits are judged as today: the
options widen only the release form.

Open points for the lead:

1. The moved pin must name the release's version (as `package.json`'s must). A consumer whose
   plugin version differs from its release subject's version would need that dropped.
2. `release-core` covers the core only. An interface change also moves every other plugin the
   repository pins (`ElsiKora/Gate:git@0.1.0` to `@0.2.0`); with this design those stay
   `#beyond-pin` findings, so such a commit needs an exception, or `release-core` widens to every
   pin.
3. One plugin per repository; a list is the extension if a repository releases several.

## Done when

- Tests, each a pass and a fail: `gate.toml` with only the plugin's source moved, and with more;
  `gate.lock` with only the plugin's entry moved, and with more; a pin moved to another version
  than the release's; a second plugin's pin moved; the core moved with and without
  `release-core`; the same pin change in a non-release (direct) commit, still reported under
  `direct`; `release-core` without `release-plugin`, and a pin file in `release-files`, as
  configuration errors.
- The rule page and the plugin changelog under `Unreleased`.
- `rustfmt --edition 2024 --check` on the changed `.rs` files exits 0 before each commit.
- fmt, clippy, test (`--no-fail-fast`), deny, build and the git plugin's wasm build exit 0 in one
  lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-049-release-pins` from `origin/main` at `105ca02`; this file first, with
the design, sent to the lead before any code.

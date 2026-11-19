# T-049: a release of git/main-history that moves a plugin's pin

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.1 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | `main` at `105ca02` |
| Touches | `plugins/git/src/main_history.rs`, `plugins/git/Cargo.toml`, `Cargo.lock`, its tests, `plugins/git/rules/main-history.md`, `plugins/git/CHANGELOG.md` |

## Goal

A repository that releases a Gate plugin of its own pins that plugin in its own `gate.toml` and
`gate.lock`, and its release commit moves the pin to the new version. Today `release-files` admits a
file whole, and only `package.json` and `package-lock.json` are limited to their version fields, so
a release commit that also changes another plugin's pin, or the core's, passes. With this task the
release commit may change the pin of that one plugin and nothing else in Gate's pin files; the other
pins (the core's and every other plugin's) may move with it only when the repository says so.

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
- `release-all-pins` (boolean, `false` by default): the same commit may also move every other pin,
  as a plugin interface change forces: the core (the root `gate.toml`'s `[gate] version` and
  `gate.lock`'s `[core]` table) and each other plugin's pin (the version in its `source`, and its
  lock entry's `url` and `sha256` under the moved key). Nothing else is admitted: options, rules,
  and a plugin added or removed stay `#beyond-pin`. It needs `release-plugin`; set alone it is a
  configuration error.

`gate.toml` and `gate.lock` are admitted by `release-plugin`, not by `release-files`: naming either
in `release-files` while `release-plugin` is set is a configuration error, since the file would be
admitted whole and limited at once. Without `release-plugin`, nothing changes. The files are
compared as parsed TOML, as `package.json` is compared as JSON, so a change of layout or comments
alone is not seen. They are read at the parent and at the commit through the plugin interface's
git reads (`read_at`), like `package.json`. Merge and direct commits are judged as today: the
options widen only the release form.

The lead's answers (2026-11-19): the moved pin of `release-plugin` must name the release's version;
`release-all-pins` replaces the design's `release-core`, since an interface change moves every pin
at once and allowing only the core would leave main red; one plugin is enough, as a release commit
has one version from its subject.

## Done when

- Tests, each a pass and a fail: `gate.toml` with only the plugin's source moved, and with more;
  `gate.lock` with only the plugin's entry moved, and with more; a pin moved to another version
  than the release's; a second plugin's pin moved; the core and another plugin moved with and
  without `release-all-pins`, and a plugin added or removed even with it; the same pin change in a
  non-release (direct) commit, still reported under `direct`; `release-all-pins` without
  `release-plugin`, and a pin file in `release-files`, as configuration errors.
- The rule page and the plugin changelog under `Unreleased`.
- `rustfmt --edition 2024 --check` on the changed `.rs` files exits 0 before each commit.
- fmt, clippy, test (`--no-fail-fast`), deny, build and the git plugin's wasm build exit 0 in one
  lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-049-release-pins` from `origin/main` at `105ca02`; this file first, with
the design, sent to the lead before any code.
2026-11-19: the lead approved the design with `release-all-pins` (every pin, not only the core) in
place of `release-core`; the moved pin names the release's version; one plugin. Code starts after
T-042 is green and merged.
2026-11-19: at the lead's word, coded before T-042 merges (to be rebased onto main after it).
`plugins/git/src/pins.rs` compares the pin files as parsed TOML: each `source` naming the plugin
(and with `release-all-pins` every plugin) loses its version, the lock's entries are re-keyed by the
plugin without its version and lose `url` and `sha256`, `release-all-pins` drops `[gate] version`
and `[core]`; then the two sides must be equal, and each moved pin of the plugin must name the
release's version. `git/main-history` admits `gate.toml` (any) and the root `gate.lock` through
`release-plugin`, checks the options at load (configuration errors), and reports `#beyond-pin`,
`#version` and `#unreadable` under `release`. The git plugin depends on the workspace `toml`
(`Cargo.lock`: one line, the dependency of `gate-plugin-git`). Tests: a unit test of the
comparison and four end-to-end tests (each pin file passing and failing, the wrong version, an
unreadable lock, a second plugin and the core with and without `release-all-pins`, a plugin
removed, the same change in a direct commit, three configuration errors). Rule page and changelog.

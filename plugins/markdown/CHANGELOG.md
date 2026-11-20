# Changelog of the markdown plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:markdown@<version>`.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Changed

- Rebuilt for the plugin interface `gate:plugin@0.3.0`. Its rules, their options and their findings
  are unchanged; pin `markdown@0.3.0` with the 0.3.0 core.

### Features

- The presets `markdown/recommended` and `markdown/adoption`: `frontmatter` and `local-links`, at
  `error` and at `warning`. They leave out `required-sections`, whose `sections` only the project
  knows.

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-markdown-v0.1.0...plugin-markdown-v0.2.0) (2026-11-19)

Built for the plugin interface `gate:plugin@0.2.0`; it loads only in a 0.2.0 core.

### Changed

- `markdown/frontmatter`, `local-links` and `required-sections` give each finding whose message
  names a line, a path or an option value a stable key (what it is about, never a line or column),
  so an edit above a finding or a reworded message keeps its fingerprint. These findings'
  fingerprints change once, with this release; `gate baseline prune` re-keys their baseline
  entries when the repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))
- A wrong option ends the run as a configuration error (exit 2, `configuration error`), not as
  an environment error. A file the rule cannot read stays an environment error. ([eaa4f37](https://github.com/ElsiKora/Gate/commit/eaa4f3770f96227855b6a1b7c438d966d9dbf24a))

# 0.1.0

- `markdown/local-links`: local link, image and reference-definition destinations of documentation
  pages name a tracked file or directory.
- `markdown/frontmatter`: documentation pages open with front matter that holds each required key.
- `markdown/required-sections`: documentation pages have the level-two sections their path requires.

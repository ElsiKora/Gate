# Changelog of the markdown plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:markdown@<version>`.

## Unreleased

### Changed

- `markdown/frontmatter`, `local-links` and `required-sections` give each finding whose message
  names a line, a path or an option value a stable key (what it is about, never a line or column),
  so an edit above a finding or a reworded message keeps its fingerprint. These findings'
  fingerprints change once, with this release; `gate baseline prune` re-keys their baseline
  entries when the repository upgrades.

# 0.1.0

- `markdown/local-links`: local link, image and reference-definition destinations of documentation
  pages name a tracked file or directory.
- `markdown/frontmatter`: documentation pages open with front matter that holds each required key.
- `markdown/required-sections`: documentation pages have the level-two sections their path requires.

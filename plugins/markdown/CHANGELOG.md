# Changelog of the markdown plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:markdown@<version>`.

## Unreleased

### Changed

- A wrong option ends the run as a configuration error (exit 2, `configuration error`), not as
  an environment error. A file the rule cannot read stays an environment error.

# 0.1.0

- `markdown/local-links`: local link, image and reference-definition destinations of documentation
  pages name a tracked file or directory.
- `markdown/frontmatter`: documentation pages open with front matter that holds each required key.
- `markdown/required-sections`: documentation pages have the level-two sections their path requires.

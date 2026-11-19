# Changelog of the ci plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:ci@<version>`.

## Unreleased

### Changed

- `ci/workflow-run-target` gives each finding whose message names a line, a path or an option value
  a stable key (what it is about, never a line or column), so an edit above a finding or a reworded
  message keeps its fingerprint. These findings' fingerprints change once, with this release: their
  baseline entries are written again once, with `gate baseline write`, when the repository upgrades.

# 0.1.0

- `ci/actions-pinned`: workflows use actions and reusable workflows pinned by a full commit SHA.
- `ci/workflow-run-target`: `workflow_run` triggers name workflows that exist.

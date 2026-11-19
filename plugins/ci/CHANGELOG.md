# Changelog of the ci plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:ci@<version>`.

## Unreleased

### Changed

- A wrong option ends the run as a configuration error (exit 2, `configuration error`), not as
  an environment error. A file the rule cannot read stays an environment error.

# 0.1.0

- `ci/actions-pinned`: workflows use actions and reusable workflows pinned by a full commit SHA.
- `ci/workflow-run-target`: `workflow_run` triggers name workflows that exist.

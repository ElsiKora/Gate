# Changelog of the ci plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:ci@<version>`.

# [0.3.0](https://github.com/ElsiKora/Gate/compare/plugin-ci-v0.2.0...plugin-ci-v0.3.0) (2026-11-21)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Breaking Changes

- `ci/actions-pinned` reports a commit pin without its full release in a comment (`# v4.2.2`), as
  the new aspect `version-comment` with the option of the same name, at the rule's level. A pin
  commented `# v4`, or not commented, that passed under 0.2.0 now fails at `error`. Write the
  release the commit is after the pin, or set
  `"ci/actions-pinned" = { levels = { version-comment = "warning" } }` while the pins are updated.
  The pin finding keeps the empty aspect, so its fingerprint is unchanged.

### Features

- The preset `ci/adoption`: the eleven rules of `ci/recommended` at `warning`, for a repository on
  its way to the standard.
- The preset `ci/recommended`: every rule of the plugin at `error`, one standard taken the same way
  in every repository. The workflow `gate sync` writes meets it.
- `ci/permissions`: no write scope in a workflow's own `permissions`, every job sets its own, no
  `read-all` or `write-all`.
- `ci/concurrency`: every workflow has a concurrency group, and `cancel-in-progress: true` only in a
  workflow that runs for pull requests alone.
- `ci/timeout`: every job that runs steps sets `timeout-minutes`, at most `max-minutes` when set.
- `ci/runner-pinned`: no `runs-on` label that matches `moving` (`*-latest` by default).
- `ci/checkout-credentials`: every checkout sets `persist-credentials: false`.
- `ci/secrets-in-steps`: no secret in a workflow's or a job's `env`, no `secrets: inherit`.
- `ci/environment`: a job that writes `contents`, `deployments`, `id-token` or `packages` runs in an
  environment.
- `ci/node-setup`: `actions/setup-node` with `node-version-file` and `cache`, once per job.
- `ci/names`: every workflow, job and step named; a workflow's file is its name in kebab case; job
  ids in kebab case; no duplicate job or workflow names.

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-ci-v0.1.0...plugin-ci-v0.2.0) (2026-11-19)

Built for the plugin interface `gate:plugin@0.2.0`; it loads only in a 0.2.0 core.

### Changed

- `ci/workflow-run-target` gives each finding whose message names a line, a path or an option
  value a stable key (what it is about, never a line or column), so an edit above a finding or a
  reworded message keeps its fingerprint. These findings' fingerprints change once, with this
  release; `gate baseline prune` re-keys their baseline entries when the repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))
- A wrong option ends the run as a configuration error (exit 2, `configuration error`), not as
  an environment error. A file the rule cannot read stays an environment error. ([eaa4f37](https://github.com/ElsiKora/Gate/commit/eaa4f3770f96227855b6a1b7c438d966d9dbf24a))

# 0.1.0

- `ci/actions-pinned`: workflows use actions and reusable workflows pinned by a full commit SHA.
- `ci/workflow-run-target`: `workflow_run` triggers name workflows that exist.

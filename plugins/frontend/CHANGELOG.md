# Changelog

All notable changes to the `frontend` plugin are recorded here. The plugin follows semantic
versioning; a rule id, aspect, option or finding message that changes is a breaking change.

# 0.3.0 (unreleased)

### Added

- The `frontend` plugin: one standard for React apps and libraries. The option `pack` names the
  framework (`react-vite`, `nextjs`, `react-library`); every rule needs it, and a missing pack is a
  configuration error that lists the three. Seventeen rules: `frontend/layout`,
  `frontend/file-names`, `frontend/typescript-sources`, `frontend/import-direction`,
  `frontend/import-paths`, `frontend/component-names`, `frontend/hook-names`,
  `frontend/state-placement`, `frontend/api-boundary`, `frontend/env-access`,
  `frontend/literal-text`, `frontend/messages`, `frontend/asset-placement`, `frontend/style-files`,
  `frontend/route-files`, `frontend/client-boundary` and `frontend/colocated-files`. Two presets:
  `frontend/recommended` (every rule at `error`) and `frontend/adoption` (every rule at `warning`).
  Nuxt and Vue are not covered.

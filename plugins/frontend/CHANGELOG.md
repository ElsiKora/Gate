# Changelog

All notable changes to the `frontend` plugin are recorded here. The plugin follows semantic
versioning; a rule id, aspect, option or finding message that changes is a breaking change.

## [0.3.1](https://github.com/ElsiKora/Gate/compare/plugin-frontend-v0.3.0...plugin-frontend-v0.3.1) (2026-11-23)

Built for the plugin interface `gate:plugin@0.3.0`; it loads in a 0.3.0, 0.3.1 or 0.3.2 core. The
findings are the same as 0.3.0's; only the time changes.

### Performance

- Its regular expressions run on `regex-automata` built with its faster engines (the lazy DFA,
  the bounded backtracker, the one-pass DFA and literal prefilters), which report the same matches.
  ([45eda36](https://github.com/ElsiKora/Gate/commit/45eda36e3e5c54bb11ab6885169898c87b76df34))
- A parsed file builds its line index when a line is first asked for, so a file that reports
  nothing and has no syntax error never builds it. ([80518fe](https://github.com/ElsiKora/Gate/commit/80518fe35472aa6a90cbdf8da5a190b5de16cfa2))

# 0.3.0 (2026-11-21)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Added

- The `frontend` plugin: one standard for React apps and libraries. The option `pack` names the
  framework (`react-vite`, `nextjs`, `react-library`); every rule needs it, and a missing pack is a
  configuration error that lists the three. Twenty rules: `frontend/layout`,
  `frontend/file-names`, `frontend/typescript-sources`, `frontend/import-direction`,
  `frontend/import-paths`, `frontend/component-names`, `frontend/hook-names`,
  `frontend/state-placement`, `frontend/api-boundary`, `frontend/env-access`,
  `frontend/literal-text`, `frontend/messages`, `frontend/asset-placement`, `frontend/style-files`,
  `frontend/route-files`, `frontend/client-boundary`, `frontend/colocated-files`,
  `frontend/import-alias`, `frontend/route-config` and `frontend/packages`. Two presets:
  `frontend/recommended` (every rule at `error`) and `frontend/adoption` (every rule at `warning`).
  Nuxt and Vue are not covered.

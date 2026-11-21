# Changelog

All notable changes to the `node` plugin are recorded here. The plugin follows semantic versioning;
a rule id, aspect, option or finding message that changes is a breaking change.

# 0.3.0 (2026-11-21)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Added

- The `node` plugin: one Node toolchain for every project of a kind (`service`, `frontend`,
  `infrastructure`, `library`, `monorepo-root`, the plugin option `kind`). It does not replace
  ESLint or Prettier; it makes their configuration and the scripts that run them the same in every
  repository, so `gate run` runs them as steps of one check. Nine rules:
  `node/package-manager` (pnpm, pinned exactly, one lockfile and workspace per install root),
  `node/node-version` (one exact version per install root in its `.nvmrc`, which every
  `engines.node` below it admits),
  `node/dependency-versions` (exact versions, through the workspace catalog where there is one),
  `node/formatter` (one `.prettierrc.json` with the standard values, and no `.editorconfig` that
  changes what Prettier writes), `node/linter` (one flat
  `eslint.config.mjs` on the shared configuration, imported by every member), `node/package`
  (the manifest fields of the kind), `node/scripts` (the standard script names; a bare name
  checks, `<name>:fix` writes), `node/tsconfig` (every configuration of a project extends its
  install root's base, turns no strict flag off, and resolves modules the kind's way) and
  `node/tsconfig-base` (each install root's base carries every strict flag, checked once per
  install whether or not a project owns it). Two presets: `node/recommended` (all nine at error)
  and `node/adoption` (all nine at warning).

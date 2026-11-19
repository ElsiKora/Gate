# Changelog of the typescript plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:typescript@<version>`.

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-typescript-v0.1.0...plugin-typescript-v0.2.0) (2026-11-19)

Built for the plugin interface `gate:plugin@0.2.0`; it loads only in a 0.2.0 core.

### Added

- `typescript/barrel-imports`: a file outside a folder with a barrel (`barrel-name`, `index.ts` by
  default) imports from that folder only through its barrel (`#deep-path`), and only names the
  barrel exports, through `export *` and renames (`#not-exported`). Kind children of an owner
  folder (`owners`) import each other directly; importers `exempt` matches are not checked. ([ce7b20c](https://github.com/ElsiKora/Gate/commit/ce7b20caf276c8e0933d20622f0018d99d493f5d))

### Changed

- `typescript/parse` gives each finding whose message names a line, a path or an option value a
  stable key (what it is about, never a line or column), so an edit above a finding or a reworded
  message keeps its fingerprint. These findings' fingerprints change once, with this release;
  `gate baseline prune` re-keys their baseline entries when the repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))
- A wrong option, or a `[languages.typescript] tsconfig` that names an untracked file, ends the run
  as a configuration error (exit 2, `configuration error`), not as an environment error. A file the
  rule cannot read stays an environment error. The language library's error carries that kind and
  converts into the SDK's error with it. ([7164aa8](https://github.com/ElsiKora/Gate/commit/7164aa823886fabb6d504701e6dc69934aca7b33))

# 0.1.0

- The TypeScript language: `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs` and `.cjs` files,
  parsed with oxc into the syntax trees other plugins' rules read.
- `typescript/parse`: every tracked TypeScript or JavaScript source file of a project parses; a file
  oxc cannot recover from, or one that is not UTF-8 text, is reported once, at the first error.

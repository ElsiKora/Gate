# Changelog of the typescript plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:typescript@<version>`.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Breaking Changes

- `typescript/string-enums` replaces `nestjs/string-enums`, and no alias keeps the old id: a
  `[rules]` entry or a profile that names `nestjs/string-enums` is a configuration error (exit 2);
  rename each one to `typescript/string-enums` and declare this plugin. Its option
  `shim-enum-names` is gone; the check is the rule `nestjs/error-string-shim`.

### Added

- `typescript/string-enums`, moved from the `nestjs` plugin: string-literal variant contracts use
  enums (`#literal-union`, `#test-literal-union`, `#raw-enum-value`, `#raw-discriminant`,
  `#keyof-typeof-alias`, `#unsupported-namespace`). It reads every tracked `.ts` file of the project
  outside `[languages.typescript] skip-folders`, so a CDK app or a script is covered as the
  application is. The new option `test-roots` (default `["test"]`) names the test folders; a root
  that is empty, absolute or not normalized is a configuration error (exit 2). The aspect
  `error-string-shim` stays in `nestjs` as the rule `nestjs/error-string-shim`.

### Features

- The presets `typescript/recommended` and `typescript/adoption`: `parse`, `barrel-imports` and
  `string-enums`, at `error` and at `warning`.

### Bug Fixes

- A project tsconfig that extends a base above the project root, such as
  `"extends": "../../tsconfig.base.json"` in a monorepo, no longer stops every rule that resolves
  imports with an environment error (exit 2). The module resolver reads each base of the `extends`
  chain through the plugin's tree, so a base outside the project is read when it is tracked and
  granted with `reads = ["tsconfig.base.json"]` in the plugin's entry, and its `paths` that lead
  back into the project resolve. A base the plugin cannot read is one finding of the call, at the
  tsconfig that names it, with the grant to add; resolution goes on without it. So are a base named
  by an absolute path, an empty entry and a `#` entry whose `package.json` `imports` target cannot
  be read. A base from an untracked package (`@tsconfig/node22`), or a `#` entry that maps to one,
  is skipped, as an import of one is.

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

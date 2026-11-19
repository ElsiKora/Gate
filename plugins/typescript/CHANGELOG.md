# Changelog of the typescript plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:typescript@<version>`.

## Unreleased

- `typescript/barrel-imports`: a file outside a folder with a barrel (`barrel-name`, `index.ts` by
  default) imports from that folder only through its barrel (`#deep-path`), and only names the
  barrel exports, through `export *` and renames (`#not-exported`). Kind children of an owner
  folder (`owners`) import each other directly; importers `exempt` matches are not checked.

# 0.1.0

- The TypeScript language: `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs` and `.cjs` files,
  parsed with oxc into the syntax trees other plugins' rules read.
- `typescript/parse`: every tracked TypeScript or JavaScript source file of a project parses; a file
  oxc cannot recover from, or one that is not UTF-8 text, is reported once, at the first error.

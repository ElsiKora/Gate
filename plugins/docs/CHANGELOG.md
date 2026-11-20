# Changelog of the docs plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:docs@<version>`.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Features

- The plugin: one documentation layout for every project, a standard per kind (`service`,
  `frontend`, `infrastructure`, `library`, `monorepo-root`), set by `kind` in the
  `[plugins.docs]` options. The standards differ only where a kind's nature demands it, each
  difference with its reason: a monorepo root has no `modules/` and no `ledger/`, and a library
  requires `releases/`.
- `docs/layout` (rule): the required entries, unknown and reserved names, declared extensions
  (`extra`), the delegated entries a project lists (`delegated`), the indexes of optional and
  declared entries, the index's entry table and its order, each index's required sections, and
  seeded pages not yet filled.
- `docs/layout` (generator): seeds the required index pages of the kind, the index linking the
  delegated entries listed; `gate sync` writes a seed only where nothing exists.
- The presets `docs/recommended` (every rule at error) and `docs/adoption` (every rule at
  warning).

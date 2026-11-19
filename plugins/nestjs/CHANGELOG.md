# Changelog

All notable changes to the `nestjs` plugin are recorded here. The plugin follows semantic versioning;
a rule id, aspect, option or finding message that changes is a breaking change.

## Unreleased

### Changed

- Every rule whose message names a line, a path or an option value gives each such finding a
  stable key: the field (`Class.member`), the class, the symbol, the import specifier, the module,
  the registry row or the service, with the check after `#` where one subject can fail several;
  never a line or column. A finding's fingerprint no longer follows its message, so an edit above
  it, a reworded message or the option file's path no longer makes a baseline entry stale. The
  fingerprints of these findings change once, with this release; `gate baseline prune` re-keys
  their baseline entries when the repository upgrades.
- A wrong option, a file the options name that is not tracked, an option that names a file the
  rule parses (`startup-file`, `registry`, `operation-enum-path`) without a JavaScript or
  TypeScript extension, or a `[languages.typescript] tsconfig` that names an untracked file, ends
  the run as a configuration error (exit 2, `configuration error`), not as an environment error. A
  tracked source or generated file that does not parse (a `package-exports-file` included) stays an
  environment error: its fix is in the tree.

# 0.1.0

- The plugin crate: the `[plugins.nestjs] options` layout block, whose schema the plugin declares so
  the core checks it when the plugin loads, the source walks and syntax helpers the rules share, and
  the two rule lists.
- The structure rules: `adapter-boundary`, `advisory-lock-boundary`, `advisory-lock-manifest`,
  `asset-storage`, `automator-metadata`, `barrel-exports`, `constant-files`, `controller-owners`,
  `declaration-files`, `dto-decorators`, `enum-values`, `error-logging`, `field-inventory`,
  `import-direction`, `notification-egress`, `package-pins`, `raw-config-access`, `runtime-factory`
  and `string-enums`.
- The semantic rules: `api-readiness`, `custom-routes`, `entity-descriptions`, `entity-table-name`,
  `eslint-directives`, `import-aliases`, `kind-layout`, `migrations`, `module-names`,
  `module-registry`, `native-functions`, `native-repository`, `owner-boundaries`, `owner-roots`,
  `prefix-nesting`, `repeated-segment`, `typeorm-relation` and `wrappers`.

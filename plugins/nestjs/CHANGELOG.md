# Changelog

All notable changes to the `nestjs` plugin are recorded here. The plugin follows semantic versioning;
a rule id, aspect, option or finding message that changes is a breaking change.

## Unreleased

### Changed

- `nestjs/field-inventory` (every aspect whose message names a path or an option value) and
  `nestjs/controller-owners` (every aspect) give each finding a stable key: the field
  (`Class.member`), the controller class, the registry row's path or the service, with the check
  after `#` where one subject can fail several. Their fingerprints no longer follow the message, so
  the option file's path or a line number in a message no longer makes a baseline entry stale. The
  fingerprints of these findings change once, with this release: their baseline entries are written
  again once, with `gate baseline write`, when the repository upgrades.

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

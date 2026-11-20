# Changelog of the cdk plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:cdk@<version>`. A rule id, aspect,
option or finding message that changes is a breaking change.

## Unreleased

### Added

- The `cdk` plugin: one standard for every AWS CDK app written in TypeScript, checked without running
  the app. Fourteen rules: `layout`, `file-names`, `declarations`, `barrels`, `app`, `environment`,
  `stack-names`, `construct-ids`, `physical-names`, `properties`, `tags`, `removal-policy`, `nag`
  and `tests`; the options `extra`, `extra-tags`, `regions` and `nag-packs`, each extension with
  its job and the reserved names refused; the presets `recommended` and `adoption`.
- `cdk/nag` wants each declared cdk-nag pack registered as a validation plugin of the app,
  `Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }))`, and reports the
  cdk-nag 2 form.
- `cdk/declarations` and `cdk/barrels` take over the checks of the CDK barrels and declaration files
  from the `nestjs` plugin, whose `layout.cdk-root` option is removed in the same release.

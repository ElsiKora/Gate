# Changelog of the cdk plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:cdk@<version>`. A rule id, aspect,
option or finding message that changes is a breaking change.

## Unreleased

### Added

- The `cdk` plugin: one standard for every AWS CDK app written in TypeScript, checked without running
  the app. Fifteen rules: `layout`, `file-names`, `declarations`, `barrels`, `app`, `environment`,
  `stack-names`, `construct-ids`, `physical-names`, `properties`, `tags`, `removal-policy`, `nag`,
  `container-health` and `tests`; the options `extra`, `extra-tags`, `regions` and `nag-packs`, each
  extension with its job and the reserved names refused; the presets `recommended` and `adoption`,
  both with every rule.
- `cdk/nag` wants cdk-nag 3 in `package.json` and each declared pack registered as a validation
  plugin of the App, `Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }))`;
  it reports a pack registered on anything but the App, an acknowledgment without a reason, and the
  cdk-nag 2 form.
- `cdk/container-health` wants a `healthCheck` on every ECS container definition, following props
  through same-file constants and helpers.
- `cdk/removal-policy` allows `DESTROY` only as one branch of a choice whose other branch keeps the
  data, and `autoDeleteObjects: true` exactly beside such a `DESTROY`.
- `cdk/declarations` reads the code of the whole app (`bin/`, `lib/`, `test/`, `scripts/`) and
  `cdk/barrels` the folders of `lib/`: they take over the checks of the CDK barrels and declaration
  files from the `nestjs` plugin, whose `layout.cdk-root` option is removed in the same release.

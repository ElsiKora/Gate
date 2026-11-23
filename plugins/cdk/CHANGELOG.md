# Changelog of the cdk plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:cdk@<version>`. A rule id, aspect,
option or finding message that changes is a breaking change.

## [0.3.1](https://github.com/ElsiKora/Gate/compare/plugin-cdk-v0.3.0...plugin-cdk-v0.3.1) (2026-11-23)

Built for the plugin interface `gate:plugin@0.3.0`; it loads in a 0.3.0, 0.3.1 or 0.3.2 core. The
findings are the same as 0.3.0's; only the time changes.

### Performance

- The TypeScript call state the rules share keeps a parsed file's bytes, so a rule that reads a
  parsed file's bytes gets them from memory instead of a second host read, and a parsed file builds
  its line index when a line is first asked for, so a file that reports nothing and has no syntax
  error never builds it. ([5f34882](https://github.com/ElsiKora/Gate/commit/5f348825ab0d8f9b0db44381f7725a606ec7346a), [80518fe](https://github.com/ElsiKora/Gate/commit/80518fe35472aa6a90cbdf8da5a190b5de16cfa2), [a15a318](https://github.com/ElsiKora/Gate/commit/a15a3180e86866a2bb59ecb7d673d32f442f64c7))

# 0.3.0 (2026-11-21)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

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

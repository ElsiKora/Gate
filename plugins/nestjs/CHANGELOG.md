# Changelog

All notable changes to the `nestjs` plugin are recorded here. The plugin follows semantic versioning;
a rule id, aspect, option or finding message that changes is a breaking change.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`.

### Breaking Changes

- The plugin holds no knowledge of AWS CDK apps; a CDK app is a project of the `cdk` plugin.
  - `layout.cdk-root` and `layout.cdk-code-folders` are removed; a configuration that sets them is a
    configuration error.
  - `barrel-exports` and `declaration-files` no longer read the CDK app (`cdk/barrels` and
    `cdk/declarations` check it).
  - `error-logging`'s default `roots` is `layout.source-root` alone, and `field-inventory`'s default
    `contract-roots` no longer names `<cdk-root>/lib/config/`.
  - `kind-layout`'s `unit-exempt-prefixes` defaults to `[]`.
  - `asset-storage`'s aspect `cloudfront-cdk-only` is `cloudfront-infrastructure-only`; its message
    says CloudFront belongs to the infrastructure-as-code project.
- `nestjs/string-enums` moves to the `typescript` plugin as `typescript/string-enums`, which reads
  every tracked `.ts` file and takes `test-roots`. No alias keeps the old id: a `[rules]` entry or a
  profile that names `nestjs/string-enums` is a configuration error (exit 2); rename each one to
  `typescript/string-enums`.
- The aspect `error-string-shim` of `nestjs/string-enums` is the rule `nestjs/error-string-shim`,
  whose option `enum-names` was `shim-enum-names`; add it to `[rules]` and to every profile that had
  `nestjs/string-enums` on.
- `nestjs/raw-config-access` reports under aspects. Its finding of 0.2 is now `boundary`; a file
  outside the boundary that reads `process.env` is `process-env`; and the keys the raw configuration
  getters and `process.env` read are compared with the project's `.env.example`: `undeclared`,
  `unread`, `dynamic`, `example-missing` and `example-syntax`, with the options `example-file`,
  `getters` and `external-keys`. No finding holds a value of the example file. A baseline entry's
  fingerprint holds the aspect: after the upgrade, check that `gate check` reports the same files
  under `boundary`, then run `gate baseline write` once.
- `nestjs/wrappers#raw-nest-exception`: a construction whose arguments read a plain parameter of
  its scope's own function (no default, no destructuring, never reassigned) has the scope form of
  the target, with the parameter in its `parameters` list:
  `src/x.ts: findListed parameters baseUrl: new BadRequestException({ baseUrl })`. Such a
  construction had the line form `<path>:<line>: <construction>`; an exception that named it by
  that line form must name it by the scope form now. A read of a local variable, of a reassigned
  or defaulted parameter or of `arguments` keeps the line form.
- `nestjs/wrappers` option `reporter-adapters`: an entry also matches a class that implements the
  named export, an interface port, not only one that extends it. Its field `class` is renamed
  `port`, with no alias: an entry with `class` is an options error until it says `port`.

## [0.2.1](https://github.com/ElsiKora/Gate/compare/plugin-nestjs-v0.2.0...plugin-nestjs-v0.2.1) (2026-11-20)

Built for the plugin interface `gate:plugin@0.2.0`; it loads in a 0.2.0 or 0.2.1 core.

### Performance

- A walk below a folder reads the paths below it, from the source's sorted range, instead of
  filtering every tracked path: a rule that walks per folder, such as `nestjs/barrel-exports` over
  each source folder, no longer reads every path once per folder. The paths walked and their order
  are the same. ([d688aee](https://github.com/ElsiKora/Gate/commit/d688aeec796cfb8f4703300ca56f8e0fcfe41cd1))

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-nestjs-v0.1.0...plugin-nestjs-v0.2.0) (2026-11-19)

Built for the plugin interface `gate:plugin@0.2.0`; it loads only in a 0.2.0 core.

### Changed

- Every rule whose message names a line, a path or an option value gives each such finding a
  stable key: the field (`Class.member`), the class, the symbol, the import specifier, the module,
  the registry row or the service, with the check after `#` where one subject can fail several;
  never a line or column. A finding's fingerprint no longer follows its message, so an edit above
  it, a reworded message or the option file's path no longer makes a baseline entry stale. The
  fingerprints of these findings change once, with this release; `gate baseline prune` re-keys
  their baseline entries when the repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))
- A wrong option, a file the options name that is not tracked, an option that names a file the
  rule parses (`startup-file`, `registry`, `operation-enum-path`) without a JavaScript or
  TypeScript extension, or a `[languages.typescript] tsconfig` that names an untracked file, ends
  the run as a configuration error (exit 2, `configuration error`), not as an environment error. A
  tracked source or generated file that does not parse (a `package-exports-file` included) stays an
  environment error: its fix is in the tree. ([d20eefa](https://github.com/ElsiKora/Gate/commit/d20eefaeca6e7a0abc1ae2b021c3b8cf2101b818))
- The rules of one call share the reads of the project's files, one parse of each TypeScript file
  and one resolution of each import, through the plugin's call state: a call reads and parses each
  file once, not once per rule that needs it. The findings are the same. ([fc44cf1](https://github.com/ElsiKora/Gate/commit/fc44cf148a31ef107e231e515553fea6cb2e59cd))

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

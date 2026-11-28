# Changelog

All notable changes to the `nestjs` plugin are recorded here. The plugin follows semantic versioning;
a rule id, aspect, option or finding message that changes is a breaking change.

## Unreleased

Built for the plugin interface `gate:plugin@0.3.0`.

### Features

- `nestjs/typeorm-lock-owner`, a new rule in no preset: every TypeORM row lock of a read takes its
  options from the one function the `owner` file exports, through the read shapes the rule
  recognises (a repository or entity-manager read of `read-methods`, a query builder's
  `setFindOptions`, and a service read of `service-read-methods`), with the repository the read runs
  on as its first argument. Aspects `builder`, `lock-object`, `lock-value`, `owner`, `receiver`,
  `row-list` and `unreadable`; options `owner`, `roots`, `framework-package`, `service-base`,
  `service-read-methods`, `step-context-method`, `read-methods` and `lock-modes`.
- `nestjs/kind-imports`, a new rule in no preset: every source and test file loads each declaration
  from the barrel of the kind folder that declares it, or from its file when it sits in no kind
  folder, in the importer's own kind folder, or behind no barrel. Aspects `kind-barrel`,
  `own-kind-folder`, `resource-relative`, `index-segment`, `dynamic-import` and `mock-target`;
  options `kinds`, `barrel-name`, `internal-folder`, `registry`, `registry-import`, `mock-calls`,
  `hoisted-mock-calls`, `entity-folder` and `enum-kind`. Its page says how it relates to
  `barrel-exports`, `import-aliases`, `migrations` and `typescript/barrel-imports`.

## [0.3.2](https://github.com/ElsiKora/Gate/compare/plugin-nestjs-v0.3.1...plugin-nestjs-v0.3.2) (2026-11-24)

Built for the plugin interface `gate:plugin@0.3.0`; it loads in a 0.3.0, 0.3.1 or 0.3.2 core.
`raw-config-access` reports fewer findings; no rule id, aspect, option or message changes.

### Bug Fixes

- `raw-config-access` names a key bound by an element callback over a literal array of strings,
  as it names the variable of a `for ... of` loop over one: the first parameter, a plain name, of an
  arrow function or function expression passed directly to `map`, `flatMap`, `forEach`, `filter`,
  `find`, `findIndex`, `findLast`, `findLastIndex`, `some` or `every` on the array, with or without
  optional chaining. Such a read is no longer `dynamic`, and the example lines of its keys are no
  longer `unread`. The first parameter of `reduce` and `reduceRight`, a callback passed by name, a
  later parameter and a method on any other value still name no key, nor does a loop variable or
  parameter that anything assigns, updates or redeclares. A read through an empty array, by a loop
  or a callback, is `dynamic` instead of no read at all. ([7a5854e](https://github.com/ElsiKora/Gate/commit/7a5854e09998f41ab2f7d284dc876f738eb0d7f0), [ed67864](https://github.com/ElsiKora/Gate/commit/ed6786498080fc934807e14d6ef4d251183113de))
- `raw-config-access` no longer counts a getter call whose first argument can never be a key (an
  object, array, function or class expression, or a numeric, bigint, boolean, `null` or
  regular-expression literal) as a key read, so a configuration store's `get({ ... })` is not
  `dynamic`. A name, call, member or other unknown expression is still `dynamic`, and `process.env`
  reads and `boundary` do not change. ([7a5854e](https://github.com/ElsiKora/Gate/commit/7a5854e09998f41ab2f7d284dc876f738eb0d7f0))

## [0.3.1](https://github.com/ElsiKora/Gate/compare/plugin-nestjs-v0.3.0...plugin-nestjs-v0.3.1) (2026-11-23)

Built for the plugin interface `gate:plugin@0.3.0`; it loads in a 0.3.0, 0.3.1 or 0.3.2 core. The
findings are the same as 0.3.0's; only the time changes.

### Performance

- The TypeScript call state the rules share keeps a parsed file's bytes, so a rule that reads a
  parsed file's bytes gets them from memory instead of a second host read, and a parsed file builds
  its line index when a line is first asked for, so a file that reports nothing and has no syntax
  error never builds it. A source root's import bindings are built once per call and shared by the
  six rule sites that used to build their own. ([5f34882](https://github.com/ElsiKora/Gate/commit/5f348825ab0d8f9b0db44381f7725a606ec7346a), [80518fe](https://github.com/ElsiKora/Gate/commit/80518fe35472aa6a90cbdf8da5a190b5de16cfa2), [a15a318](https://github.com/ElsiKora/Gate/commit/a15a3180e86866a2bb59ecb7d673d32f442f64c7))
- Its regular expressions run on `regex-automata` built with its faster engines (the lazy DFA,
  the bounded backtracker, the one-pass DFA and literal prefilters), which report the same matches.
  ([45eda36](https://github.com/ElsiKora/Gate/commit/45eda36e3e5c54bb11ab6885169898c87b76df34))
- The costliest rules lose repeated work: `advisory-lock-boundary` tests registered identifiers
  against a set of the file's numbers, builds each file's interpreter once, passes non-digits at
  once in its literal scan and checks UTF-8 once, with FxHash for its sets and maps;
  `module-registry` reads each chapter's source map once; `raw-config-access` uses FxHash for its
  maps; `asset-storage` computes a file's tokens once and borrows the names of the syntax tree.
  ([b836856](https://github.com/ElsiKora/Gate/commit/b836856eafa493edce988bbcfbfe9465ca274a07), [9f0054f](https://github.com/ElsiKora/Gate/commit/9f0054fbf1db231a96dfd5574776bed2c858ea23), [60a1469](https://github.com/ElsiKora/Gate/commit/60a14694129ec2c93a967e350761511d763ceab6), [55d7322](https://github.com/ElsiKora/Gate/commit/55d7322013572620470073407d652c6c5e102d27), [542150f](https://github.com/ElsiKora/Gate/commit/542150f1ee1faa9ba4cfb3a9f4282bc33417b0c4), [570c94a](https://github.com/ElsiKora/Gate/commit/570c94a3e61fd85db496988b451b580c811ea7e4), [fbb2fa4](https://github.com/ElsiKora/Gate/commit/fbb2fa44ef4919caf08ec676a6da99ae036d292b))
- On a large tree (about 11 thousand tracked files) the plugin's fuel went from 57.04e9 to 34.07e9
  (-40.3 %) and its host reads from 15267 to 10789; its call, the critical path of a cold check
  without the history rules, took 2843 to 3960 ms in seven cold runs against 6572 ms on 0.3.0.

# [0.3.0](https://github.com/ElsiKora/Gate/compare/plugin-nestjs-v0.2.1...plugin-nestjs-v0.3.0) (2026-11-21)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

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
  its scope's own function (no default, no destructuring, never reassigned, not redeclared) has the
  scope form of the target, with the parameter in its `parameters` list:
  `src/x.ts: findListed parameters baseUrl: new BadRequestException({ baseUrl })`. Such a
  construction had the line form `<path>:<line>: <construction>`; an exception that named it by
  that line form must name it by the scope form now. A read of a local variable, of a defaulted,
  reassigned, destructured or redeclared parameter or of `arguments` keeps the line form.
- `nestjs/wrappers` option `reporter-adapters`: an entry also matches a class that implements the
  named export, an interface port, not only one that extends it. Its field `class` is renamed
  `port`, with no alias: an entry with `class` is an options error until it says `port`.

### Features

- The presets `nestjs/recommended` and `nestjs/adoption`: 25 of the 37 rules, at `error` and at
  `warning`. The twelve they leave out take something only the project can give, such as an option
  without a default or its own inventory; the plugins guide lists them with the reason.

### Bug Fixes

- `nestjs/barrel-exports` no longer reports the entry point NestJS starts from: unset,
  `private-entrypoints` approves the source root's file named by `entrypoint-suffix`,
  `<layout.source-root>/main.ts` by default (`src/main.ts`), where it approved none and every
  service's `src/main.ts` was a `#runtime-entrypoint` error. A list replaces the default, and `[]`
  approves none. The help names `private-entrypoints` as an option of the rule's `[rules]` entry.
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

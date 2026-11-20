# T-064: a cdk plugin, one standard for every AWS CDK app

| Field | Value |
| --- | --- |
| Status | done (merged at `0a3d5a8`) |
| Release | v0.3.0, the final release; the plugin must be in it |
| Writer | "Gate 002" |
| Reviewer | a separate review session |
| Depends on | `gate:plugin@0.3.0` (the plugin is built against the interface v0.3.0 ships); T-062 for the `infrastructure` documentation kind, which this plugin does not check |
| Touches | a new `plugins/cdk` (fourteen rules, two presets, their pages, the changelog), `Cargo.toml` (the workspace member), `Cargo.lock` (by hand), `docs/guide/plugins.md` (the plugin's row and section); `plugins/nestjs` (`layout.cdk-root` and `layout.cdk-code-folders` removed, `string-enums` moved out; section 6); `plugins/typescript` (`string-enums` moved in) |

## Goal

Every AWS CDK app written in TypeScript follows one standard: the same file layout, the same names
(stacks, constructs, construct ids, files, exported names), the same structure, the same handling of
accounts, configuration, tags and removal. A reader who knows one app finds their way in any other.
One plugin, `cdk`, checks everything about it that can be checked without running the app: from the
tracked paths, the JSON files and the TypeScript syntax tree. It never runs `cdk synth`, never reads
the cloud, and never needs credentials.

## What was read for this design

Three CDK apps in three repositories, plus an older copy of one of them in a fourth. Everything was
read read-only, by listing tracked files and reading files at `HEAD`. Nothing was run in any of them.
No name, path or text of theirs is in this repository; the examples below are synthetic.

**They come from one template and have already drifted apart.** All four share 129 paths once the
project's name is taken out of them: one entry file, `cdk.json`, the same `lib/` directories (config,
construct, stack, naming, validation, deployment, synthesizer), a naming guard, an example
configuration, the same lint and compiler settings. The older copy differs from its newer one only in a
handful of files, so the comparison is between three apps.

| Aspect | App A | App B | App C (closest to the standard) |
| --- | --- | --- | --- |
| Account and region | from the CLI's default environment, read through reflection; one account literal in a constant | the target account and region hardcoded as constants; a second account literal | from an external configuration file; no account literal |
| Configuration | per-environment values in code (`lib/config/environments/<env>/`) | the same | one JSON file passed as context; an example of it tracked |
| Project prefix | a "brand" context value | a "project" value | a "project" value |
| Policy checks | cdk-nag 2 as an aspect | the same | cdk-nag 3 as a validation plugin, with acknowledgments |
| Dependency versions | caret ranges | caret ranges | exact pins (a workspace catalog) |
| `cdk.json` app command | without `npx` | with `npx` | with `npx` |
| Script names | `synth:<deploy-dir>:<env>…` | `synth:<env>…` | `synth:example:offline`, `synth:provider` |
| Example configuration folder | `deploy/<name A>/` | `deploy/<name B>/` | `deploy/example/` |
| Guard rule ids | one scheme | another scheme | as B |
| Unconditional `RemovalPolicy.DESTROY` | 2 sites | 2 sites | none |
| Unit tests | none (synth scripts only) | none (synth scripts only) | configuration tests, but outside the app |

Shared by all three, the template's own drift: `props` and `properties` both used for construct
properties; most files carry a role suffix (`.construct.ts`, `.interface.ts`) but stacks, deployments
and a few functions do not; a stack's directory and file name its topic differently (`data/` holds
`…-aurora-postgres-stack.ts`, `cache/` holds `…-redis-stack.ts`); only some stack directories have an
`index.ts`; some files are named only by their role (`…/name-tag/function.ts`); one tag key is lower
case beside Pascal-case ones; every stack, construct and deployment file and class carries the
project's name.

What all three already do well and the standard keeps: physical names only through naming functions
(no literal physical name in any app), Pascal-case construct ids (above 98 percent in each), one tag
function applying the same keys, the naming guard, strict compiler settings.

## 1. The standard

Paths are relative to the CDK app's root, the directory holding `cdk.json`. **The app is a Gate project
of its own**, so its root is the project root: a repository lists it in `projects` (for example
`infrastructure/cdk`). There is no option naming a folder inside a project. Its documentation is the
T-062 `infrastructure` kind, checked by the `docs` plugin, not by this one.

### Layout

```text
cdk.json                      the app command and the CDK feature flags, nothing else
package.json                  exact versions of the CDK packages
tsconfig.json
bin/application.ts            the only entry: reads the configuration, builds the app
lib/
  config/                     the configuration's type, reader and parser; no values
  naming/                     physical names, construct names, stack names, tags
  validation/                 checks of the configuration and of the synthesized app
  deployment/                 the stage that puts one environment's stacks together
  stack/<topic>/              one stack per directory
    <topic>.stack.ts
    index.ts
    interface/properties.interface.ts
    interface/index.ts
  construct/<topic>/          the constructs of one topic
    <what>.construct.ts
    index.ts
  synthesizer/                the stack synthesizer
  aspect/                     aspects (optional)
guard/naming.guard            the policy rules every template must pass
deploy/<environment>.config.example.json   an example configuration; the real one is not tracked
scripts/                      the scripts `package.json` runs
test/                         unit tests, mirroring lib/: test/stack/<topic>/<topic>.stack.test.ts
```

**One name per job, in the same spirit as T-062.** Each job has one directory, singular, the same in
every app. A project adds a `lib/` directory only by declaring it with its job (`extra`). These names
are reserved and refused even as an extension, because their job has a directory:

| Reserved | Goes to |
| --- | --- |
| `stacks`, `constructs`, `configs`, `aspects` | `stack`, `construct`, `config`, `aspect` |
| `configuration`, `environments`, `settings` | `config` (the type and reader) and `deploy/` (the values) |
| `names`, `tags`, `tagging` | `naming` |
| `stages`, `deployments` | `deployment` |
| `utils`, `util`, `helpers`, `common`, `shared` | the topic that uses it, as a `.utility.ts` |

### Names

- **Files** are `<name>.<role>.ts` in kebab case: `network.stack.ts`, `bucket-policy.construct.ts`,
  `create-physical-name.function.ts`. The roles are fixed: `stack`, `construct`, `deployment`,
  `aspect`, `synthesizer`, `interface`, `type`, `enum`, `constant`, `function`, `utility`,
  `configuration`, `validation`, `error`. `index.ts` and `bin/application.ts` are the only files
  without a role. `<name>` is never empty and never a role itself.
- **No project prefix** in file, directory or class names. The project's name is data: it reaches
  stack names, physical names and tags through the naming functions, from the configuration. Two apps
  then have the same paths and class names for the same job.
- **Classes**: a file `<name>.<role>.ts` whose role is a class role (stack, construct, deployment,
  aspect, synthesizer, error) exports exactly one class, `Pascal(<name>) + Pascal(<role>)`:
  `network.stack.ts` exports `NetworkStack`, `environment.deployment.ts` exports
  `EnvironmentDeployment`.
- **Stacks**: the stack's directory is its topic, and the file and class repeat it:
  `lib/stack/aurora-postgres/aurora-postgres.stack.ts` exports `AuroraPostgresStack`. The construct id
  of a stack is built by a naming function from the project, the environment and the topic, and its
  `stackName` is the same value: `AcmeStagingAuroraPostgres`.
- **Construct ids** are Pascal case, `^[A-Z][A-Za-z0-9]*$`, as a string literal, or a template
  literal whose text parts keep that form (`Subnet${index}`). Within one constructor, no two literal
  ids are equal.
- **Physical names** (any `*Name` property of a construct's properties that AWS uses as a resource's
  name, `exportName` included) are only ever the result of a function exported from `lib/naming/`.
- **Declarations**: a file declares one top-level entity, and its kind matches the file's role: a
  class in a class-role file, an interface in `.interface.ts`, a type alias in `.type.ts`, an enum
  in `.enum.ts`. No class, interface, type alias or enum is declared below the top level.
- **Barrels**: each folder below `lib/` that holds a `.ts` file has an `index.ts`; it holds only
  export declarations, and re-exports, by a relative specifier without its extension, each `.ts`
  file of its folder and each child folder that has an `index.ts`, and nothing outside its folder.
- **Properties**: a stack's or construct's constructor is `(scope, id, properties)`. The third
  parameter is named `properties`, its type is `I<Class>Properties`, and it is declared in
  `interface/properties.interface.ts` of the same directory. The word `props` is not used.

### Environment, account and configuration

- No account id in the tree: no 12-digit literal in TypeScript or in a tracked JSON file, except the
  all-zero placeholder an offline synth uses, and except `cdk.context.json`: the CDK writes its
  lookups there, each key naming the account, and AWS asks that the file be committed.
- No region literal, except the regions a project allows (`regions`), by default `us-east-1` for the
  certificates and other resources a CDN requires there.
- The account, the region and every per-environment value come from one configuration file, named by
  the one context key `configuration` (`cdk synth -c configuration=deploy/staging.config.json`). The
  app reads no other context key and no `process.env` outside `lib/config/` (`process.exitCode` and
  the other members of `process` are not the environment).
- `cdk.json`'s `context` holds only CDK feature flags (keys with a `:`, such as
  `@aws-cdk/core:newStyleStackSynthesis`), never a project value.
- `deploy/` tracks only `<environment>.config.example.json`. A real configuration is never tracked.

### Tags, removal, entry point, dependencies, tests

- **Tags** are applied in one place, `lib/naming/`, with the keys `Project`, `Environment`,
  `ManagedBy`, `Application` and `Name`, all Pascal case. A project adds a key only by declaring it
  with its job (`extra-tags`).
- **Removal**: `RemovalPolicy.DESTROY` appears only as one branch of a choice, a conditional that
  selects between `DESTROY` and a policy that keeps the data (`RETAIN`, `RETAIN_ON_UPDATE_OR_DELETE`,
  `SNAPSHOT`): the other branch of a conditional expression or of an `if` with an `else`, or another
  case of a `switch`. A default (`?? RemovalPolicy.DESTROY`) and an `if` with no keeping branch are
  not choices. `autoDeleteObjects: true` follows its sibling `removalPolicy`: it is allowed exactly
  when that `DESTROY` is. A tree that destroys data unconditionally destroys it in production too.
- **Entry point**: `cdk.json`'s `app` is exactly `npx ts-node --prefer-ts-exts bin/application.ts`;
  `bin/` holds only `application.ts`; the tree constructs one `App`, there.
- **Policy checks**: `package.json` depends on cdk-nag 3 (3.0.2 is the latest on npm), and
  `bin/application.ts` registers each cdk-nag pack the project declares (`nag-packs`, by default
  `AwsSolutionsChecks`) in the cdk-nag 3 form of its README, as a validation plugin of the App:
  `Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }))`, `app` the
  `const` bound to `new App()`, never a stack or a stage. The cdk-nag 2 form
  (`Aspects.of(app).add(new AwsSolutionsChecks())`) and its `NagSuppressions` are not used; a finding
  is acknowledged with `Validations.of(construct).acknowledge({ id, reason })`, the reason given.
- **Containers**: every ECS container definition (`taskDefinition.addContainer(id, props)`,
  `new ContainerDefinition(scope, id, props)`) has a `healthCheck`, so ECS replaces a container that
  stops answering.
- **Dependencies**: `aws-cdk-lib`, `aws-cdk`, `constructs` and `cdk-nag` are exact
  versions in `package.json` (`2.180.0`, not `^2.180.0`), or a workspace catalog reference
  (`catalog:`). `package.json` has the scripts `synth`, `test` and `lint`.
- **Tests**: each stack has a unit test, `test/stack/<topic>/<topic>.stack.test.ts`, and each file of
  `lib/naming/` and `lib/config/` with a function role has one beside its mirror path.

## 2. The rules

Fifteen rules, each with aspects, project scope. **Path** means the rule reads only the tracked paths;
**JSON** that it also reads JSON files; **AST** that it reads the TypeScript syntax tree.

| Rule | Reads | Aspects | Checks | Why |
| --- | --- | --- | --- | --- |
| `cdk/layout` | Path | `missing`, `unknown`, `extra-missing`, `stack-directory`, `entry` | the required files and directories exist; every `lib/` entry is a standard one or declared (a reserved one's finding says where it goes); a declared one exists; each `lib/stack/<topic>/` has its stack file, `index.ts` and properties interface; `bin/` holds only `application.ts` | one tree, so a reader and a tool find the same thing in the same place |
| `cdk/file-names` | Path | `case`, `role`, `role-place`, `bare-role` | kebab-case segments; a known role suffix; `stack` only under `lib/stack/`, `construct` only under `lib/construct/`, a file in a role directory (`interface/`) carries that role; no file named only by its role | the role is visible from the path, and two apps name the same file the same way |
| `cdk/app` | JSON | `command`, `context`, `pin`, `scripts` | `cdk.json`'s app command; `cdk.json`'s context holds only feature flags; the CDK packages pinned exactly; the required scripts | the same command, flags and versions build every app the same way |
| `cdk/environment` | AST, JSON | `account`, `region`, `context-key`, `process-env`, `tracked-configuration` | no account literal (TypeScript and tracked JSON); no region literal outside `regions`; only the `configuration` context key; no `process.env` outside `lib/config/`; no real configuration tracked | an account in code deploys to the wrong place when it is copied, and environment values belong in configuration |
| `cdk/declarations` | AST | `count`, `name`, `kind`, `nested` | one top-level entity per code file of `bin/`, `lib/`, `test/` and `scripts/`, none below the top level; in `lib/`, a class-role file's class named from its file and the entity's kind matching the role | the entity is found from its file and the file from its entity |
| `cdk/barrels` | AST | `missing`, `reexport-only`, `explicit-export`, `outside` | each `lib/` folder with a `.ts` file has `index.ts`; it holds only export declarations; it re-exports each file and child folder; it re-exports nothing outside its folder | a folder's public surface is one file, and it hides no code; only `lib/` is imported, so only `lib/` has barrels |
| `cdk/stack-names` | AST | `topic`, `id`, `stack-name` | the stack's directory, file and class name one topic; the stack id is a call to a `lib/naming/` function with that topic; `stackName` is the same expression | one stack, one name, from one function |
| `cdk/construct-ids` | AST | `case`, `duplicate` | of each construct instantiation (`new X(this \| scope, id, …)`, `X` from `aws-cdk-lib` or `constructs` or an app class that extends one): a Pascal-case id, of a template its literal parts; no repeated literal id for one scope in one function, unless the two are in branches that exclude each other | the id is part of every logical id and path; a changed case replaces the resource |
| `cdk/physical-names` | AST | `literal` | of each construct instantiation, each physical-name property, `exportName` included, is the result of a `lib/naming/` function, not a string built in place (a literal, a template, a concatenation, another call, or a same-file `const` of one); the props are read in place or from a same-file `const` | names stay unique per project and environment and follow one scheme |
| `cdk/properties` | AST | `parameter`, `interface` | the constructor's third parameter is `properties`, typed `I<Class>Properties` from `interface/properties.interface.ts` | one word and one place for a construct's input |
| `cdk/tags` | AST | `place`, `key`, `required` | `Tags.of(…).add` only in `lib/naming/`; each key is standard or declared, a key that is not a literal read from the same-file map a loop takes it from; each standard key is applied; an unreadable key gives no finding and turns `required` off | cost, ownership and policy tools read the same keys in every account |
| `cdk/removal-policy` | AST | `destroy`, `auto-delete` | `DESTROY` only as one branch of a choice whose other branch keeps the data; `autoDeleteObjects: true` only beside such a `DESTROY` | data is never destroyed by a deploy that did not choose to |
| `cdk/nag` | AST, JSON | `missing`, `pack`, `form`, `version`, `target`, `acknowledgment` | cdk-nag 3 in `package.json`; `bin/application.ts` registers a cdk-nag pack as a validation plugin of the App, in place or through a same-file `const`; each declared pack is registered; no pack registered on anything but the App; every acknowledgment gives a reason; no cdk-nag 2 form (`Aspects.of(…).add` of a pack, `NagSuppressions`) | cdk-nag is the stack's security check, and the v3 form runs it on the whole app after every aspect |
| `cdk/container-health` | AST | `missing` | every ECS container definition has a `healthCheck`; the props are followed through same-file `const`s, a same-file helper that returns them and one that passes them through; props that cannot be read give no finding | ECS replaces a hung container only when it has a health check |
| `cdk/tests` | Path | `stack`, `module` | each stack's unit test; each function file of `lib/naming/` and `lib/config/` has its test | the naming scheme and the configuration parser are what every stack depends on |

Example findings:

```text
error cdk/stack-names#topic lib/stack/data/aurora-postgres.stack.ts
  lib/stack/data/aurora-postgres.stack.ts: the stack's directory is data but its file is aurora-postgres; one topic names both
error cdk/environment#account lib/validation/target.constant.ts:4
  the string literal is a 12-digit account id; read the account from the configuration file
error cdk/removal-policy#destroy lib/construct/storage/artifact-bucket.construct.ts:31
  RemovalPolicy.DESTROY is unconditional; make it one branch of a choice that keeps the data otherwise
```

**What a rule does not do.** No rule checks a generic TypeScript convention: string enums are
`typescript/string-enums`, and a file that does not parse is `typescript/parse`'s finding only; a cdk
rule skips it. Barrels and declarations are checked here because the standard fixes them for a CDK
app (section 6). No rule reads `docs/`. No rule runs the app or reads a synthesized template.

**The checks that need the syntax tree** (eleven rules) link the shared crate `gate-typescript`
(`languages/typescript`, built on oxc), as the `nestjs` and `typescript` plugins do. It gives one
parse per file per run, module resolution with the tsconfig paths, import bindings (to tell
`aws-cdk-lib`'s `Stack` and `RemovalPolicy` from a local name) and static value folding (to read a
template literal's text parts). Reusing the `typescript` plugin's parser "through the SDK" is not
possible: the SDK exposes no parser, and plugins do not call each other at run time. Sharing the crate
is the existing way; each component carries its own copy of the parser, as `nestjs` does.

**The checks that are only path and JSON checks** (four rules): `cdk/layout`, `cdk/file-names`,
`cdk/app`, `cdk/tests`, and the `tracked-configuration` aspect of `cdk/environment` and its JSON
account check. A project that wants only these pays no parse.

## 3. Options: fixed, and what a project may extend

Everything in section 1 is a constant of the plugin. A project extends it only where the standard
cannot know the answer, and every extension carries its job, as T-062's `extra` does:

```toml
[plugins]
cdk = { source = "ElsiKora/Gate:cdk@0.3.0", options = { extra = [{ name = "dashboard", job = "CloudWatch dashboards built from the stacks' metrics" }], extra-tags = [{ name = "CostCenter", job = "The finance code the account's bill is split by" }] } }
```

| Option | Default | Extends |
| --- | --- | --- |
| `extra` | `[]` | `lib/` directories, `{ name, job }`: kebab case, singular, not a standard or reserved name |
| `extra-tags` | `[]` | tag keys, `{ name, job }`: Pascal case, not a standard key |
| `regions` | `["us-east-1"]` | the region literals allowed |
| `nag-packs` | `["AwsSolutionsChecks"]` | the cdk-nag packs the app registers, each one cdk-nag 3.0.2 exports (`AwsSolutionsChecks`, `HIPAASecurityChecks`, `NIST80053R4Checks`, `NIST80053R5Checks`, `PCIDSS321Checks`, `ServerlessChecks`, read from the package's npm tarball); an empty list or an unknown pack is a configuration error |

A broken extension (a reserved name, a standard name, a case error, an empty job) is a configuration
error at load (exit 2), naming the key. A declared `lib/` directory that does not exist is
`cdk/layout#extra-missing`. There is no option to rename a standard directory, change a role, allow a
project prefix or switch the configuration's source: those are what the standard holds the same.

The rules have no options of their own beyond their levels; they read the plugin's.

## 4. Presets

- `recommended`: every rule at `error`.
- `adoption`: `recommended` with the aspects that rename files and classes at `warning`
  (`cdk/file-names`, `cdk/declarations`, `cdk/barrels`, `cdk/properties`, `cdk/stack-names#topic`,
  `cdk/tests`), so
  an existing app can turn the plugin on, fix the account, configuration and removal findings first,
  and rename in its own commits. The aspects that protect a deploy (`cdk/environment`,
  `cdk/removal-policy`, `cdk/physical-names`, `cdk/nag`, `cdk/container-health`) stay `error` in
  both.

## 5. Tests

Native SDK tests, one fixture per aspect with a passing twin, on synthetic trees; one end-to-end run
through the built component.

- **`cdk/layout`**: the standard tree passes; each required entry absent, one at a time, is one
  `missing`; `lib/stacks/` is `unknown` naming `lib/stack`, `lib/utils/` naming the `.utility.ts`
  form; a declared `lib/dashboard/` passes and an absent one is `extra-missing`; a stack directory
  without `index.ts` or its properties interface is `stack-directory`; `bin/deploy.ts` is `entry`.
  `docs/`, `AGENTS.md` and `gate.toml` at the root are never reported.
- **`cdk/file-names`**: `NetworkStack.ts` and `network_stack.ts` are `case`; `network-stack.ts` and
  `network.thing.ts` are `role`; `network.stack.ts` under `lib/construct/` is `role-place`, as is
  `x.constant.ts` in an `interface/` directory; `name-tag/function.ts` is `bare-role`; `index.ts`
  passes.
- **`cdk/app`**: the standard command passes, `ts-node bin/application.ts` fails; a `brandName`
  context key fails beside passing feature flags; `^2.180.0` fails, `2.180.0` and `catalog:` pass; a
  missing `test` script fails.
- **`cdk/environment`**: a 12-digit literal in a constant, in a template literal's text and in a
  tracked JSON file each fail, the all-zero placeholder passes; `eu-west-1` fails, `us-east-1` passes, and
  passes when `regions` lists it; `tryGetContext("brandName")` fails, `"configuration"` passes;
  `process.env.CDK_DEFAULT_ACCOUNT` in `bin/` fails, in `lib/config/` passes, `process.exitCode = 1`
  passes; `deploy/staging.config.json` tracked fails, `staging.config.example.json` passes; a
  `cdk.context.json` lookup key that embeds an account passes; `cn-north-1` and `us-gov-west-1` are
  regions, reported unless `regions` lists them.
- **`cdk/declarations`**: `network.stack.ts` exporting `NetworkStack` passes, `AcmeNetworkStack`
  fails `name`; two classes, or a class and an exported function, fail `count`; an interface in a
  `.constant.ts` and a class in an `.interface.ts` fail `kind`; an interface inside a function fails
  `nested`; `index.ts` is not judged; a test file with two interfaces fails `count` and a script
  with a nested interface fails `nested`.
- **`cdk/barrels`**: a `lib/construct/network/` folder without `index.ts` fails `missing`; an
  `index.ts` with a `const` fails `reexport-only`; one that misses a file or a child folder with an
  index fails `explicit-export`; `export * from "../other"` and a re-export of `bin/application`
  fail `outside`; the standard barrels pass.
- **`cdk/nag`**: `Validations.of(app).addPlugins(new AwsSolutionsChecks(app))` passes, and so does a
  pack bound to a `const` first; no registration fails `missing`; `nag-packs = ["AwsSolutionsChecks",
  "ServerlessChecks"]` with only the first registered fails `pack`; `Aspects.of(app).add(new
  AwsSolutionsChecks())` and a `NagSuppressions.addStackSuppressions` call fail `form`; cdk-nag
  `2.37.55` fails `version` and the message names the v3 form; `Validations.of(deployment)` fails
  `target`; a local `Validations` registers nothing; `acknowledge` without a `reason` or with a blank
  one fails `acknowledgment`; `nag-packs = ["Unknown"]` exits 2.
- **`cdk/container-health`**: `addContainer` and `new ContainerDefinition` with a `healthCheck` pass,
  in place, through a `const`, a spread, a helper that returns the props and a helper the props pass
  through; without one each fails `missing`, the pass-through at its caller's line; props from an
  imported call, or a spread of one, give no finding; `aws_ecs.ContainerDefinition` of `aws-cdk-lib`
  is judged.
- **`cdk/stack-names`**: `lib/stack/data/aurora-postgres.stack.ts` fails `topic`; an id written as a
  literal or a template, not through `lib/naming/`, fails `id`; a naming call with another topic fails
  `id`; `stackName` different from the id fails; the standard stack passes.
- **`cdk/construct-ids`**: `"VpcEndpoints"` and `` `Subnet${index}` `` pass; `"vpc-endpoints"`,
  `"vpcEndpoints"` and `` `subnet${index}` `` fail `case`; two `"Bucket"` ids in one constructor fail
  `duplicate`, the same id in two constructors passes, and so does the same id in the two branches of
  an `if`/`else` or a conditional; `` `${prefix}Archive` `` passes and `` `${prefix}-archive` ``
  fails; `new RegExp(pattern, "u")` and `new Date(value, "utc")` are not constructions.
- **`cdk/physical-names`**: `bucketName: "acme-assets"`, a template, a concatenation, a call of
  another function and a same-file `const` of a literal fail, and so does a template in a props
  object bound to a same-file `const`; `bucketName: createPhysicalName(…)` imported from
  `lib/naming/` passes.
- **`cdk/properties`**: `props` as the parameter fails; `IBucketProps` fails; the interface declared
  in the stack file fails; the standard passes.
- **`cdk/tags`**: `Tags.of(this).add` in a construct fails `place`; `application` fails `key`, a
  declared `CostCenter` passes; a naming module that never applies `ManagedBy` fails `required`; a
  loop over `Object.entries(tags)` of a same-file map with an undeclared `Team` fails `key` at the
  call, and a loop over a map that cannot be read gives no finding at all.
- **`cdk/removal-policy`**: an unconditional `DESTROY`, `disposable ? DESTROY : DESTROY`,
  `policy ?? RemovalPolicy.DESTROY` and a `DESTROY` in an `if` with no `else` fail;
  `disposable ? DESTROY : RETAIN` and an `if`/`else` with `SNAPSHOT` pass; `autoDeleteObjects: true`
  passes beside that conditional and fails beside an unconditional `DESTROY`, beside `RETAIN` and
  beside no policy; a local `RemovalPolicy` not imported from `aws-cdk-lib` is not judged.
- **`cdk/tests`**: a stack without its test fails, a naming function without its test fails, an
  interface file needs none.
- **Options**: each broken `extra` and `extra-tags` entry exits 2 naming the key.
- **Parse**: a file that does not parse gives no cdk finding.
- **Presets**: both set every rule and only aspects their rules declare; `recommended` is all
  `error`; `adoption` warns on `file-names`, `declarations`, `barrels`, `properties`, `tests` and
  `stack-names#topic` only.
- **End to end**: the built component, given with `--plugin`, reports a container without a health
  check and an unchosen `DESTROY` and exits 1. The standard app passing every rule is a native test.

## The lead's answers (2026-11-19)

1. **No project prefix** in file and class names: one name per job; the repository already says
   which project it is.
2. **`nestjs` keeps no CDK knowledge.** 0.3.0 is breaking and final: `layout.cdk-root` and
   `layout.cdk-code-folders` leave `nestjs` in this release, each piece of what they do moves to
   `cdk` or, where it is a generic TypeScript convention, to `typescript` (section 6), under
   `Breaking Changes`.
3. **A stack's chapter in `docs/modules/`** belongs to the `docs` plugin (T-062), not here.
4. **cdk-nag is a rule** (`cdk/nag`): the app registers it in the standard's form, the cdk-nag 3
   validation plugin, with the packs the project declares.

## The lead's rulings on the review of `ebe468f` (2026-11-20)

1. **`cdk/container-health`** is a rule: every ECS container definition of `aws-cdk-lib/aws-ecs`
   (`taskDefinition.addContainer(id, props)`, `new ContainerDefinition(scope, id, props)`) has a
   `healthCheck`. Props are followed through a same-file `const` or object literal and a same-file
   helper that builds or passes them through; props that cannot be read statically give no finding.
   The `taskImageOptions` of `aws-ecs-patterns` are left out: its type,
   `ApplicationLoadBalancedTaskImageOptions` in aws-cdk-lib 2.277.0, has no container health check
   (read from the package's `.d.ts`); a pattern gets one through its `taskDefinition`. In both
   presets.
2. **cdk-nag 3**, in the form of its README, `Validations.of(app).addPlugins(new
   AwsSolutionsChecks(app))`; 3.0.2 is the latest on npm. `package.json` on cdk-nag before 3 is
   `cdk/nag#version`, whose message names the v3 form. The six pack class names are read from the
   cdk-nag 3.0.2 npm tarball.
3. **`cdk.context.json`** is committed, as AWS asks, and each lookup key in it names the account:
   `cdk/environment#account` does not read it.
4. **Removal**: a choice is a conditional that selects between `DESTROY` and `RETAIN`/`SNAPSHOT`;
   `?? RemovalPolicy.DESTROY` and an `if` with no keeping alternative are reported;
   `autoDeleteObjects: true` is allowed exactly when its sibling `DESTROY` is.
5. **Tags**: a key that is not a literal is read from a same-file object literal map; a map that
   cannot be read gives no finding.
6. **Nag registration**: a same-file `const` bound to `new XChecks(…)` counts; `Validations.of(…)`
   takes the App, the variable bound to `new App()`, and anything else is `target`; an
   acknowledgment with an empty or missing `reason` is `acknowledgment`.
7. **Construct ids** judge construct instantiations only: `new X(this | scope, id, …)`, `X` from
   `aws-cdk-lib` or extending `Construct`; of a template only the literal parts' case; the same id in
   mutually exclusive branches is no duplicate.
8. The tests of section 5 that the first draft missed: a file that does not parse, the adoption
   preset, the end-to-end run, and the failing cases of each filter.
9. **Physical names**: any string expression built in place is reported: a concatenation, a
   template, a same-file `const`, and a props object built in a same-file variable.
10. **Regions** include `cn-*` and `us-gov-*`.
11. **`process-env`** reports reads of `process.env` only.
12. **Coverage** `cdk-root` gave: `cdk/declarations` reads the whole app (`bin/`, `lib/`, `test/`,
    `scripts/`); `cdk/barrels` stays on `lib/`, the only folder anything imports (section 6).
13. **Migration notes** in the root changelog: `[[exception]]` entries naming old ids are a
    configuration error; baselines; `layout.test-root` becomes `typescript/string-enums`
    `test-roots`. A root `Features` entry announces the plugin.

## 6. What `nestjs`'s `cdk-root` does today, and where each piece goes

| Piece | Today, in `nestjs` | Goes to |
| --- | --- | --- |
| The options | `layout.cdk-root` (`"infrastructure/cdk"`) and `layout.cdk-code-folders` (`["bin", "lib", "scripts"]`) | removed; a CDK app is a project of its own with the `cdk` plugin |
| `barrel-exports` | `#reexport-only` and `#private-entrypoint` read the barrels below `cdk-root` | `cdk/barrels` (`reexport-only`, `outside`), with the standard's `missing` and `explicit-export`, on `lib/` only: a barrel serves importers, and nothing imports `bin/application.ts` (the CDK runs it), the tests (the test runner runs them) or the scripts (run by hand) |
| `declaration-files` | reads the `.ts` files below `cdk-root` (one owner per file, nested and kind-folder declarations) | `cdk/declarations`, in the standard's terms, on the whole app (`bin/`, `lib/`, `test/`, `scripts/`): one entity per file and none nested everywhere, its kind from the file's role in `lib/`, where the file name gives a role |
| `string-enums` | reads the code folders of `cdk-root` | the rule moves to `typescript/string-enums`, which reads every tracked `.ts` file of a project; `nestjs/string-enums` is removed. Its test-root distinction becomes the option `test-roots` (default `["test"]`). Its aspect `error-string-shim` checks NestJS-Crud-Automator's `ErrorString` enums, framework knowledge, so it stays in `nestjs` as the rule `nestjs/error-string-shim` (option `enum-names`, was `shim-enum-names`) |
| `error-logging` | the code folders of `cdk-root` are among its default `roots` | dropped, not moved: the rule checks the NestJS logger and `TypeORM` options, which a CDK app has neither of |
| `field-inventory` | `<cdk-root>/lib/config/` is among the default `contract-roots` | dropped from the default; `contract-roots` stays a list of paths a project may set |
| `kind-layout` | `unit-exempt-prefixes` defaults to `["test/unit/infrastructure/cdk/"]` | the default becomes `[]` |
| `asset-storage` | the aspect `cloudfront-cdk-only` (the service's code loads no CloudFront package) | renamed `cloudfront-infrastructure-only`; it checks the service's code, so it stays |
| The `layout` page | the skip-list paragraph on the walk below `cdk-root` and `cdk.out` | removed |

`nestjs`'s changelog lists the removed options, the moved rule and the renamed aspect under
`Breaking Changes`, with what a consumer changes: list the CDK app as its own project with the `cdk`
plugin, and enable `typescript/string-enums` in place of `nestjs/string-enums`.

Coordination: the open branches of Gate 006 (T-047, T-057) touch no file of `plugins/nestjs`; asked
on 2026-11-19 about unpushed work. T-059 changes `plugins/nestjs/tests/{host,large,support}/mod.rs`,
which this task leaves alone where it can.

## Done when

- The code goes on the tip that carries `gate:plugin@0.3.0`, followed by one lane round on the pushed
  tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny, the release build and the plugins'
  wasm builds exit 0, and the leak scan prints nothing.
- The plugin's pages (one per rule), its changelog and its row and section in `docs/guide/plugins.md`;
  the `nestjs` and `typescript` changelogs and pages for section 6.
- A comparison on the surveyed apps, run outside this repository: the findings each rule reports
  there match the divergences found by hand.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: design on `task/T-064-cdk-plugin` (`6c88ee9`), approved by the lead with the four
answers above. Code on a fresh branch, `task/T-064-cdk`, from `main` at `ec66737`, carrying this
file only: fourteen rules (`cdk/declarations` and `cdk/barrels` take the pieces of `nestjs`'s
`cdk-root`, `cdk/nag` is the lead's fourth answer), section 6 for `nestjs`.
2026-11-20T02:40:00+00:00: code on `task/T-064-cdk`, rebased on `main` at `e9e1434`: the plugin
(`plugins/cdk`, fourteen rules, options, presets), its tests (`plugins/cdk/tests/rules`), its pages and
changelog, the guide section; section 6 done in `nestjs` and `typescript` (`string-enums` moved,
`nestjs/error-string-shim` added). Next: the first lane round, with cdk among the wasm builds,
then its fixes.
2026-11-20T02:44:10+00:00: round 1 at `3286ab7` stopped at clippy (an unused import, `nonminimal_bool`,
`collapsible_match`); fixed. The 0.3.0 Breaking Changes for the moved and new rule ids are in the root,
`nestjs` and `typescript` changelogs, and an end-to-end test proves `nestjs/string-enums` is now an
unknown-rule configuration error. Neither `nestjs` nor `typescript` ships a preset, so the two rules
join none; asked the lead. Next: round 2.
2026-11-20T03:29:48+00:00: round 3 at `585c3e6`: fmt, clippy, deny, build and every wasm build pass. Tests failed in four
targets, all on test expectations (the SDK's rule prefix on errors, a fixture named only by roles,
the typescript rule count, a walk order), fixed in `382b9c3` and `6a2b24a`. Next: round 4.
2026-11-20T04:41:36+00:00: review fix round on `ebe468f`, rebased on `main` at `340cf8b`: the
thirteen rulings above are in the code, the pages, the changelogs and this file
(`cdk/container-health` added, fifteen rules; `cdk/nag` wants cdk-nag 3 and the App; choices in
`cdk/removal-policy`; maps in `cdk/tags`; construct-only ids and names; `cdk.context.json`,
`cn-*`, `process.env`; `cdk/declarations` on the whole app). Each new test was written to fail
with its fix removed. Next: round 5, then the report to the lead.
2026-11-20T09:16:58+00:00: round 7 at `87d0afa`, on this task's own target directory: fmt, clippy,
test (1135 passed, 0 failed, over 98 test binaries), deny, build and every wasm build exit 0. The
log compiles `gate`, `gate-plugin-cdk`, `gate-plugin-typescript` and `gate-plugin-nestjs` from this
checkout and lists the twenty new tests by name. Each new test was written so that it fails with
its fix removed; that was reasoned from the code, not run as a mutation. Next: the lead's review.

# T-064: a cdk plugin, one standard for every AWS CDK app

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead's decision; no code |
| Release | v0.3.0 if the code is green before the v0.3.0 cut; else the next release. A new plugin breaks nothing, and it releases by its own tag. |
| Writer | "Gate 002" |
| Reviewer | a separate review session |
| Depends on | `gate:plugin@0.3.0` (the plugin is built against the interface v0.3.0 ships); T-062 for the `infrastructure` documentation kind, which this plugin does not check |
| Touches | a new `plugins/cdk` (eleven rules, two presets, their pages, the changelog), `Cargo.toml` (the workspace member), `Cargo.lock` (by hand), `docs/guide/plugins.md` (the plugin's row and section) |

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
- **Properties**: a stack's or construct's constructor is `(scope, id, properties)`. The third
  parameter is named `properties`, its type is `I<Class>Properties`, and it is declared in
  `interface/properties.interface.ts` of the same directory. The word `props` is not used.

### Environment, account and configuration

- No account id in the tree: no 12-digit literal in TypeScript or in a tracked JSON file, except the
  all-zero placeholder an offline synth uses.
- No region literal, except the regions a project allows (`regions`), by default `us-east-1` for the
  certificates and other resources a CDN requires there.
- The account, the region and every per-environment value come from one configuration file, named by
  the one context key `configuration` (`cdk synth -c configuration=deploy/staging.config.json`). The
  app reads no other context key and no `process.env` outside `lib/config/`.
- `cdk.json`'s `context` holds only CDK feature flags (keys with a `:`, such as
  `@aws-cdk/core:newStyleStackSynthesis`), never a project value.
- `deploy/` tracks only `<environment>.config.example.json`. A real configuration is never tracked.

### Tags, removal, entry point, dependencies, tests

- **Tags** are applied in one place, `lib/naming/`, with the keys `Project`, `Environment`,
  `ManagedBy`, `Application` and `Name`, all Pascal case. A project adds a key only by declaring it
  with its job (`extra-tags`).
- **Removal**: `RemovalPolicy.DESTROY` and `autoDeleteObjects: true` appear only as one branch of a
  conditional whose other branch keeps the data (`RETAIN`, `RETAIN_ON_UPDATE_OR_DELETE`,
  `SNAPSHOT`, `false`). A tree that destroys data unconditionally destroys it in production too.
- **Entry point**: `cdk.json`'s `app` is exactly `npx ts-node --prefer-ts-exts bin/application.ts`;
  `bin/` holds only `application.ts`; the tree constructs one `App`, there.
- **Dependencies**: `aws-cdk-lib`, `aws-cdk`, `constructs` and, when present, `cdk-nag` are exact
  versions in `package.json` (`2.180.0`, not `^2.180.0`), or a workspace catalog reference
  (`catalog:`). `package.json` has the scripts `synth`, `test` and `lint`.
- **Tests**: each stack has a unit test, `test/stack/<topic>/<topic>.stack.test.ts`, and each file of
  `lib/naming/` and `lib/config/` with a function role has one beside its mirror path.

## 2. The rules

Eleven rules, each with aspects, project scope. **Path** means the rule reads only the tracked paths;
**JSON** that it also reads JSON files; **AST** that it reads the TypeScript syntax tree.

| Rule | Reads | Aspects | Checks | Why |
| --- | --- | --- | --- | --- |
| `cdk/layout` | Path | `missing`, `unknown`, `extra-missing`, `stack-directory`, `entry` | the required files and directories exist; every `lib/` entry is a standard one or declared (a reserved one's finding says where it goes); a declared one exists; each `lib/stack/<topic>/` has its stack file, `index.ts` and properties interface; `bin/` holds only `application.ts` | one tree, so a reader and a tool find the same thing in the same place |
| `cdk/file-names` | Path | `case`, `role`, `role-place`, `bare-role` | kebab-case segments; a known role suffix; `stack` only under `lib/stack/`, `construct` only under `lib/construct/`, a file in a role directory (`interface/`) carries that role; no file named only by its role | the role is visible from the path, and two apps name the same file the same way |
| `cdk/app` | JSON | `command`, `context`, `pin`, `scripts` | `cdk.json`'s app command; `cdk.json`'s context holds only feature flags; the CDK packages pinned exactly; the required scripts | the same command, flags and versions build every app the same way |
| `cdk/environment` | AST, JSON | `account`, `region`, `context-key`, `process-env`, `tracked-configuration` | no account literal (TypeScript and tracked JSON); no region literal outside `regions`; only the `configuration` context key; no `process.env` outside `lib/config/`; no real configuration tracked | an account in code deploys to the wrong place when it is copied, and environment values belong in configuration |
| `cdk/class-names` | AST | `name`, `count` | a class-role file exports one class, named from its file | the class is found from its file and the file from its class |
| `cdk/stack-names` | AST | `topic`, `id`, `stack-name` | the stack's directory, file and class name one topic; the stack id is a call to a `lib/naming/` function with that topic; `stackName` is the same expression | one stack, one name, from one function |
| `cdk/construct-ids` | AST | `case`, `duplicate` | Pascal-case ids; no repeated literal id in one constructor | the id is part of every logical id and path; a changed case replaces the resource |
| `cdk/physical-names` | AST | `literal` | each physical-name property, `exportName` included, is the result of a `lib/naming/` function | names stay unique per project and environment and follow one scheme |
| `cdk/properties` | AST | `parameter`, `interface` | the constructor's third parameter is `properties`, typed `I<Class>Properties` from `interface/properties.interface.ts` | one word and one place for a construct's input |
| `cdk/tags` | AST | `place`, `key`, `required` | `Tags.of(…).add` only in `lib/naming/`; each key is standard or declared; each standard key is applied | cost, ownership and policy tools read the same keys in every account |
| `cdk/removal-policy` | AST | `destroy`, `auto-delete` | `DESTROY` and `autoDeleteObjects: true` only in a conditional whose other branch keeps the data | data is never destroyed by a deploy that did not choose to |
| `cdk/tests` | Path | `stack`, `module` | each stack's unit test; each function file of `lib/naming/` and `lib/config/` has its test | the naming scheme and the configuration parser are what every stack depends on |

Example findings:

```text
error cdk/stack-names#topic lib/stack/data/aurora-postgres.stack.ts
  lib/stack/data/aurora-postgres.stack.ts: the stack's directory is data but its file is aurora-postgres; one topic names both
error cdk/environment#account lib/validation/target.constant.ts:4
  "123456789012" is an account id; read the account from the configuration file
error cdk/removal-policy#destroy lib/construct/storage/artifact-bucket.construct.ts:31
  RemovalPolicy.DESTROY is unconditional; make it one branch of a choice that keeps the data otherwise
```

**What a rule does not do.** No rule checks a generic TypeScript convention: barrels, string enums
and declaration files are the `typescript` and `nestjs` plugins' business, and a file that does not
parse is `typescript/parse`'s finding only; a cdk rule skips it. No rule reads `docs/`. No rule runs
the app or reads a synthesized template.

**The checks that need the syntax tree** (eight rules) link the shared crate `gate-typescript`
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

A broken extension (a reserved name, a standard name, a case error, an empty job) is a configuration
error at load (exit 2), naming the key. A declared `lib/` directory that does not exist is
`cdk/layout#extra-missing`. There is no option to rename a standard directory, change a role, allow a
project prefix or switch the configuration's source: those are what the standard holds the same.

The rules have no options of their own beyond their levels; they read the plugin's.

## 4. Presets

- `recommended`: every rule at `error`.
- `adoption`: `recommended` with the aspects that rename files and classes at `warning`
  (`cdk/file-names`, `cdk/class-names`, `cdk/properties`, `cdk/stack-names#topic`, `cdk/tests`), so
  an existing app can turn the plugin on, fix the account, configuration and removal findings first,
  and rename in its own commits. The aspects that protect a deploy (`cdk/environment`,
  `cdk/removal-policy`, `cdk/physical-names`) stay `error` in both.

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
  tracked JSON file each fail, `"000000000000"` passes; `eu-west-1` fails, `us-east-1` passes, and
  passes when `regions` lists it; `tryGetContext("brandName")` fails, `"configuration"` passes;
  `process.env.CDK_DEFAULT_ACCOUNT` in `bin/` fails, in `lib/config/` passes;
  `deploy/staging.config.json` tracked fails, `staging.config.example.json` passes.
- **`cdk/class-names`**: `network.stack.ts` exporting `NetworkStack` passes, `AcmeNetworkStack`
  fails, two classes fail `count`; an `.interface.ts` is not judged.
- **`cdk/stack-names`**: `lib/stack/data/aurora-postgres.stack.ts` fails `topic`; an id written as a
  literal or a template, not through `lib/naming/`, fails `id`; a naming call with another topic fails
  `id`; `stackName` different from the id fails; the standard stack passes.
- **`cdk/construct-ids`**: `"VpcEndpoints"` and `` `Subnet${index}` `` pass; `"vpc-endpoints"`,
  `"vpcEndpoints"` and `` `subnet${index}` `` fail `case`; two `"Bucket"` ids in one constructor fail
  `duplicate`, the same id in two constructors passes; a `new` whose first argument is not the scope
  (`new Date(…)`) is not judged.
- **`cdk/physical-names`**: `bucketName: "acme-assets"` and an `exportName` template fail;
  `bucketName: createPhysicalName(…)` imported from `lib/naming/` passes; the same call to a local
  function of the same name fails.
- **`cdk/properties`**: `props` as the parameter fails; `IBucketProps` fails; the interface declared
  in the stack file fails; the standard passes.
- **`cdk/tags`**: `Tags.of(this).add` in a construct fails `place`; `application` fails `key`, a
  declared `CostCenter` passes; a naming module that never applies `ManagedBy` fails `required`.
- **`cdk/removal-policy`**: an unconditional `DESTROY` and `applyRemovalPolicy(RemovalPolicy.DESTROY)`
  fail; `disposable ? DESTROY : RETAIN` passes; `autoDeleteObjects: true` fails,
  `autoDeleteObjects: disposable` passes; a local `RemovalPolicy` not imported from `aws-cdk-lib` is
  not judged.
- **`cdk/tests`**: a stack without its test fails, a naming function without its test fails, an
  interface file needs none.
- **Options**: each broken `extra` and `extra-tags` entry exits 2 naming the key.
- **Parse**: a file that does not parse gives no cdk finding.
- **Presets**: `adoption` reports the renaming aspects as warnings and the account finding as an error.
- **End to end**: a synthetic app with one finding per rule exits 1 with exactly those findings; the
  standard app exits 0.

## Open, for the lead and the owner

1. **No project prefix in names.** All three apps prefix every file and class with the project's
   name. The standard drops it, so two apps have the same paths; the alternative is a required
   `prefix` option that every file and class must carry. Dropping it is the larger rename.
2. **The `nestjs` plugin's `cdk-root`.** Today `nestjs` applies its TypeScript conventions (barrels,
   string enums, declaration files, error logging) to a CDK app below `layout.cdk-root`. With an app
   as its own project, that reading would stop. Recommended: those conventions belong to the
   `typescript` plugin, for every TypeScript project; until they move there, `cdk-root` stays, and
   the `cdk` plugin does not re-check them.
3. **A stack's chapter in `docs/modules/`.** T-062's `infrastructure` kind has a chapter per stack.
   A check that each stack has one would cross from this plugin into the `docs` plugin's tree; it is
   left out.
4. **cdk-nag.** The newest app registers cdk-nag 3 as a validation plugin. Requiring its registration
   in `bin/application.ts` ties the rule to one major version's API; it is left to review.

## Done when

- The lead decides the design and the open items. Then the code goes on the tip that carries
  `gate:plugin@0.3.0`, followed by one lane round on the pushed tip: fmt, clippy, test
  (`--workspace --no-fail-fast`), deny, the release build and the plugins' wasm builds exit 0, and the
  leak scan prints nothing.
- The plugin's pages (one per rule), its changelog and its row and section in `docs/guide/plugins.md`.
- A comparison on the surveyed apps, run outside this repository: the findings each rule reports
  there match the divergences found by hand.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-064-cdk-plugin` from `main` at `ec66737`; this file, design only, from a
read-only survey of three CDK apps and an older copy of one. Waiting for the lead's decision before
any code.

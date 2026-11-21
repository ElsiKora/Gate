# The `cdk` plugin

One standard for every AWS CDK app written in TypeScript. The app is a project of its own, its root
the directory of `cdk.json`. The rules read the tracked files only; none runs the app, `cdk synth` or
the package manager.

| Rule | What it holds true |
| --- | --- |
| [`cdk/layout`](layout.md) | The app has the standard's files and directories, and below `lib/` only those and the declared ones |
| [`cdk/file-names`](file-names.md) | Every file below `lib/` is `<name>.<role>.ts` in kebab case, in the place of its role |
| [`cdk/declarations`](declarations.md) | A code file declares one top-level entity; below `lib/` it is of its role's kind, named from the file |
| [`cdk/barrels`](barrels.md) | Each folder below `lib/` with TypeScript files has an `index.ts` that only re-exports each of them |
| [`cdk/app`](app.md) | `cdk.json` runs the standard command with feature flags only, and `package.json` pins the CDK packages |
| [`cdk/environment`](environment.md) | Accounts, regions and environment values come from the configuration file, never from the code |
| [`cdk/stack-names`](stack-names.md) | A stack's directory, file, class and id name one topic, through a `lib/naming/` function |
| [`cdk/construct-ids`](construct-ids.md) | Construct ids are Pascal case, and one scope gives no two constructs the same id |
| [`cdk/physical-names`](physical-names.md) | Resource names and export names come from a `lib/naming/` function |
| [`cdk/properties`](properties.md) | A construct takes `(scope, id, properties)`, typed `I<Class>Properties` from its `interface/` folder |
| [`cdk/tags`](tags.md) | Tags are applied in `lib/naming/` only, with the standard's keys, each of them applied |
| [`cdk/removal-policy`](removal-policy.md) | Data is destroyed only on a branch of a choice whose other branch keeps it |
| [`cdk/nag`](nag.md) | The app registers each declared cdk-nag 3 pack as a validation plugin of the App |
| [`cdk/container-health`](container-health.md) | Every ECS container definition has a health check |
| [`cdk/tests`](tests.md) | Each stack and each naming and configuration function has a unit test at its mirror path |

## The standard

```text
cdk.json                         app: npx ts-node --prefer-ts-exts bin/application.ts
package.json                     aws-cdk-lib, aws-cdk, constructs, cdk-nag 3 pinned; synth, test, lint
tsconfig.json
guard/naming.guard               the naming rules for CloudFormation Guard
deploy/<env>.config.example.json the example configuration; deploy/<env>.config.json is not tracked
bin/application.ts               the one entry: reads the configuration, builds the deployment, registers cdk-nag
lib/config/                      the configuration's type, reader and parser
lib/naming/                      physical names, construct names, stack names and tags
lib/deployment/                  the stage that puts one environment's stacks together
lib/stack/<topic>/               <topic>.stack.ts, index.ts, interface/properties.interface.ts, interface/index.ts
lib/construct/                   the constructs, by topic (optional)
lib/validation/                  checks of the configuration and of the synthesized app (optional)
lib/synthesizer/, lib/aspect/    the stack synthesizer and the aspects (optional)
test/                            the unit tests, at the mirror path of lib/
```

A file name ends with one of the roles `stack`, `construct`, `deployment`, `aspect`, `synthesizer`,
`interface`, `type`, `enum`, `constant`, `function`, `utility`, `configuration`, `validation` and
`error`. The tag keys are `Project`, `Environment`, `ManagedBy`, `Application` and `Name`. The app
registers cdk-nag as a validation plugin:

```ts
Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }));
```

## Options

Every rule reads `[plugins.cdk] options`. A project extends the standard only here, and every
extension says what it is for.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `extra` | array of `{ name, job }` | `[]` | Directories below `lib/` beyond the standard's: kebab case, neither a standard nor a reserved name, each with one sentence of what it holds. |
| `extra-tags` | array of `{ name, job }` | `[]` | Tag keys beyond the standard's: Pascal case, not a standard key, each with its job. |
| `regions` | array of string | `["us-east-1"]` | The region literals the code may hold. |
| `nag-packs` | array of string | `["AwsSolutionsChecks"]` | The cdk-nag packs `bin/application.ts` registers: `AwsSolutionsChecks`, `HIPAASecurityChecks`, `NIST80053R4Checks`, `NIST80053R5Checks`, `PCIDSS321Checks`, `ServerlessChecks`. |

The reserved names below `lib/`, refused even in `extra`, and where their content goes: `stacks`,
`constructs`, `configs` and `aspects` (the singular directory); `configuration`, `environments` and
`settings` (`lib/config` and `deploy/`); `names`, `tags` and `tagging` (`lib/naming`); `stages` and
`deployments` (`lib/deployment`); `utils`, `util`, `helpers`, `common` and `shared` (the topic that
uses it, as a `.utility.ts` file); `constants` (the topic that uses it, as a `.constant.ts` file).

A value that breaks these is a configuration error (exit 2) that names the key and the value.

```toml
[plugins.cdk]
source = "ElsiKora/Gate:cdk@0.3.0"
options = { regions = ["eu-central-1", "us-east-1"], extra = [{ name = "monitoring", job = "the alarms and dashboards" }] }
reads = ["tsconfig.base.json"]
```

`reads` grants the plugin the monorepo root's `tsconfig.base.json`, which the app's `tsconfig.json`
extends as [`node/tsconfig`](../../node/rules/tsconfig.md) wants. The module resolver reads the
base only through that grant; without it the resolver runs without the base's `paths`, and the
plugin reports the base as a finding with the line to add. An app that is the whole repository
needs no grant. See [configuration.md](../../../docs/guide/configuration.md#languages).

## Presets

- `recommended`: every rule at `error`.
- `adoption`: every rule of `recommended` at `warning`, for an existing app on its way to the
  standard; its `[rules]` raises a rule to `error` once the app passes it.

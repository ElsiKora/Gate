# `cdk/nag`

The app registers each declared cdk-nag pack as a validation plugin, in the cdk-nag 3 form.

## What it checks and why

cdk-nag is the security check of the stack: its packs judge the synthesized app against AWS's and the
compliance frameworks' rules. Registered as a validation plugin of the app, it reports on every
synthesis, and its findings are acknowledged in the app's own validation report.

```ts
Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }));
```

- `missing`: `bin/application.ts` registers no pack as a validation plugin.
- `pack`: a pack declared in `nag-packs` is not registered in `bin/application.ts`.
- `form`: a file of `bin/` or `lib/` uses the cdk-nag 2 form: a pack added as an aspect,
  `Aspects.of(x).add(new AwsSolutionsChecks(…))`, or `NagSuppressions`.

Scope: project. Reads history: no. Aspects: `missing`, `pack`, `form`. Language: TypeScript.

## Options

None of its own; it reads `nag-packs` from `[plugins.cdk] options`.

```toml
[plugins.cdk]
source = "ElsiKora/Gate:cdk@0.3.0"
options = { nag-packs = ["AwsSolutionsChecks", "ServerlessChecks"] }
```

## Example finding

```text
error cdk/nag#pack bin/application.ts
  bin/application.ts does not register the declared cdk-nag pack ServerlessChecks
```

## How to fix it

Register each declared pack with `Validations.of(app).addPlugins(…)` in `bin/application.ts`, and
acknowledge a finding with `Validations.of(construct).acknowledge(…)` instead of `NagSuppressions`.

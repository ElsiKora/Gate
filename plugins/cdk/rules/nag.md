# `cdk/nag`

The app registers each declared cdk-nag 3 pack as a validation plugin of the App.

## What it checks and why

cdk-nag is the security check of the stack: its packs judge the synthesized app against AWS's and the
compliance frameworks' rules. Registered as a validation plugin of the app, it reports on every
synthesis, and its findings are acknowledged in the app's own validation report.

```ts
Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }));
```

- `version`: `package.json` depends on cdk-nag before 3. cdk-nag 3 is the version with the
  validation-plugin form; the finding names it.
- `missing`: `bin/application.ts` registers no pack as a validation plugin of the App.
- `pack`: a pack declared in `nag-packs` is not registered in `bin/application.ts`.
- `target`: a pack is registered with `Validations.of(x)` where `x` is not the App, the `const`
  bound to `new App(…)` of `aws-cdk-lib`: a stack or a stage, for instance.
- `acknowledgment`: `Validations.of(x).acknowledge(…)` gives an entry without a `reason`, or with an
  empty one.
- `form`: a file of `bin/` or `lib/` uses the cdk-nag 2 form: a pack added as an aspect,
  `Aspects.of(x).add(new AwsSolutionsChecks(…))`, or `NagSuppressions`.

`Validations`, `Aspects` and `App` are those of `aws-cdk-lib`, and a pack is one of the six classes
cdk-nag 3.0.2 exports, constructed in place or bound to a `const` of the same file. A pack the
rule cannot read this way does not count as registered. A file that does not parse gives no
finding; `typescript/parse` reports it.

Scope: project. Reads history: no. Aspects: `missing`, `pack`, `form`, `version`, `target`,
`acknowledgment`. Language: TypeScript.

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

Upgrade cdk-nag to 3. Register each declared pack on the App with
`Validations.of(app).addPlugins(…)` in `bin/application.ts`, and acknowledge a finding with
`Validations.of(construct).acknowledge({ id, reason })` instead of `NagSuppressions`, saying in
`reason` why the finding is accepted.

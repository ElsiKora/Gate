# `cdk/environment`

Accounts, regions and environment values come from the configuration file, never from the code.

## What it checks and why

An account id or a region written in the code deploys one environment's resources into another's
account, and a value read from the process environment differs between a laptop and CI. The app
reads one configuration file, named by the context key `configuration`, and `lib/config/` turns it
into a typed value.

- `account`: a run of exactly twelve digits, not all zeros, in a tracked JSON file (outside
  `node_modules`, `package-lock.json` and `cdk.context.json`) or in a string literal or a literal
  part of a template of `bin/`, `lib/` or `scripts/`. `cdk.context.json` holds the CDK's lookups,
  each key naming the account, and AWS asks that it be committed, so the rule does not read it.
- `region`: a literal of `bin/`, `lib/` or `scripts/` holds a region not listed in `regions`. The
  China (`cn-north-1`) and GovCloud (`us-gov-west-1`) regions count.
- `context-key`: `tryGetContext` or `getContext` in `bin/` or `lib/` reads a key other than
  `configuration`.
- `process-env`: code of `bin/` or `lib/` outside `lib/config/` reads `process.env`. Other
  members of `process`, such as `process.exitCode = 1`, pass.
- `tracked-configuration`: a JSON file in `deploy/` other than `<environment>.config.example.json`
  is tracked.

Scope: project. Reads history: no. Aspects: `account`, `region`, `context-key`, `process-env`,
`tracked-configuration`. Language: TypeScript.

## Options

None of its own; it reads `regions` from `[plugins.cdk] options`.

```toml
[plugins.cdk]
source = "ElsiKora/Gate:cdk@0.3.0"
options = { regions = ["eu-central-1", "us-east-1"] }
```

## Example finding

```text
error cdk/environment#context-key bin/application.ts:8
  bin/application.ts reads the context key stage; the app reads only `configuration`, the configuration file
```

## How to fix it

Move the account, the region and the value into the configuration file and read them from the typed
configuration. Ignore `deploy/*.config.json` and keep an example beside it; list a region the code
must name, such as the one CloudFront certificates live in, in `regions`.

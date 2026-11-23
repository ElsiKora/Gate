# `cdk/layout`

The app has the standard's files and directories, and below `lib/` only those and the declared ones.

## What it checks and why

Every app in a repository looks the same, so a reader finds the configuration, the names and the
stacks in one place, and a new kind of code gets a directory that says what it is for.

- `missing`: a required file (`cdk.json`, `package.json`, `tsconfig.json`, `bin/application.ts`,
  `guard/naming.guard`) or a required directory (`lib/config`, `lib/naming`, `lib/deployment`,
  `lib/stack`) is not tracked, or `deploy/` has no `<environment>.config.example.json`.
- `unknown`: a file directly in `lib/`, or a directory below `lib/` that is neither the standard's
  nor declared in `extra`. A reserved name says where its content goes.
- `extra-missing`: a directory declared in `extra` holds no tracked file.
- `stack-directory`: a file directly in `lib/stack/`, or a stack directory without
  `<topic>.stack.ts`, `index.ts`, `interface/properties.interface.ts` or `interface/index.ts`.
- `entry`: a file in `bin/` other than `bin/application.ts`.

Scope: project. Reads history: no. Aspects: `missing`, `unknown`, `extra-missing`,
`stack-directory`, `entry`.

## Options

None of its own; it reads `extra` from `[plugins.cdk] options`.

```toml
[plugins.cdk]
source = "ElsiKora/Gate:cdk@0.3.1"
options = { extra = [{ name = "monitoring", job = "the alarms and dashboards" }] }
```

## Example finding

```text
error cdk/layout#unknown lib/utils
  lib/utils is a reserved name; its content goes to the topic that uses it, as a .utility.ts file
```

## How to fix it

Add the missing file or directory as the standard describes it. Move a file out of `lib/` or out of
an unknown directory into the directory of its job, or declare a new directory in `extra` with one
sentence of what it holds. Give each stack a directory of its own with its barrel and its properties
interface, and keep `bin/` to the one entry file.

# `nestjs/migrations`

The generated `TypeORM` migrations are fresh-only classes named by timestamp, and the generated barrel
and the registry constant register them in timestamp order.

## What it checks and why

A migration that can be rolled back invites a schema nobody tested, and a migration list out of order
runs changes against a schema they were not written for. The rule keeps the generated migrations
forward-only and makes their order the order of their timestamps everywhere they are listed.

A migration is a tracked `.ts` file directly in `generated-folder`, other than `barrel` and
`index.ts`; the walk skips none of `[languages.typescript] skip-folders`. When the tree has no
migration, no `registry` and no `barrel`, the rule reports nothing.

- `filename`: the file is named `<timestamp>-<SemanticName>.ts`, where the timestamp has exactly
  `timestamp-digits` digits and the semantic name starts with an ASCII capital letter followed by
  ASCII letters and digits. The expected class is `<SemanticName><timestamp>`.
- `timestamp`: no two migrations share a timestamp; the later file is reported, and both stay in the
  order.
- `class`: the file exports its expected class at the top level as a plain class (no `extends`,
  `implements`, `abstract` or `declare`, so nothing can supply a downgrade), with no member keyed
  `down` and with an instance property `name: string = "<class>"`. A file without that class is
  reported once, at the file.
- `baseline` (with `single-baseline` only): exactly one generated migration, the baseline, exists.
  Every later migration is reported at its file; no migration at all is reported at
  `generated-folder`.
- `barrel`: `barrel` equals, byte for byte, one line `export { <Class> } from "./<file stem>";` per
  migration in timestamp order. A missing barrel is reported.
- `registry`: `registry` imports every migration, by value, from `registry-import`, and declares
  `registry-constant` as an array literal listing them in timestamp order. A missing registry is
  reported.

A migration or registry without a syntax tree is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `barrel`, `baseline`, `class`,
`filename`, `registry`, `timestamp`. Findings carry no target.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `generated-folder` | string | `"src/migration/typeorm/generated"` | The folder of the generated migrations. |
| `barrel` | string | `"src/migration/typeorm/generated/index.ts"` | The barrel that re-exports every migration. |
| `registry` | string | `"src/migration/typeorm/migrations.ts"` | The module that lists the migrations. |
| `registry-constant` | string | `"TYPEORM_MIGRATIONS_CONSTANT"` | The registry's array of migrations. |
| `registry-import` | string | `"./generated"` | The specifier the registry imports the migrations from. |
| `timestamp-digits` | integer, at least 1 | `13` | The digits of a migration timestamp. |
| `single-baseline` | boolean | `false` | Keep exactly one generated migration, changed in place, as before a first release. |

```toml
[rules]
"nestjs/migrations" = { level = "error", options = { single-baseline = true } }
```

## Example finding

```text
error nestjs/migrations#registry src/migration/typeorm/migrations.ts:10
  TYPEORM_MIGRATIONS_CONSTANT does not match the timestamp-ordered generated migrations
```

## How to fix it

Regenerate the migration with the migration tool instead of editing its name or class by hand, then
regenerate the barrel and list the class in the registry in timestamp order. Remove any `down` member,
base class or interface. With `single-baseline`, fold the change into the baseline migration instead
of adding a new one.

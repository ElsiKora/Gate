# `nestjs/advisory-lock-manifest`

The advisory-lock manifest, operation enum and static-key markers agree.

## What it checks and why

PostgreSQL advisory locks are keyed by two signed 32-bit numbers. When two parts of a service pick the
same pair by accident they block each other, and when locks are taken out of order they deadlock. The
registry manifest assigns every lock family its identifiers, scope, rank and admitted operations, so
keys are unique and ordered; a static-key marker in the source points at the resource a static key
belongs to.

When `manifest-path` is not tracked, the rule reports one `#manifest` finding at that path and checks
nothing else.

- `#manifest`: the manifest has a carriage return, is not JSON, or breaks its schema. The header is
  exactly `schemaVersion` (`1`), `resourceDomain` (not empty) and a non-empty `families`, in that order.
  A family is exactly `name`, `familyId`, `scope`, `resourceKind`, `rank`, `predecessors`,
  `allowedOperations` and `resources`, in that order: an upper-case name, a signed int32 id, both
  unique; `scope` `PRODUCTION` with a positive integer rank or `NON_PRODUCTION` with a `null` rank;
  `resourceKind` `DYNAMIC` (non-empty operations, no resources) or `STATIC` (no operations, at least one
  resource); distinct upper-case predecessors of the same scope and a lower rank. A resource is exactly
  `name`, `resourceId` and `allowedOperations`, with a name unique in its family, an id unique in the
  manifest and non-empty distinct operations. Operations are those of `operations`. The first finding
  of each family is reported; predecessors are compared once every family is valid. Reported at the
  manifest.
- `#operation-enum`: `operation-enum-path` is missing, or is not exactly `export enum
  <operation-enum-name>` with one member per entry of `operations`, in order, each a bare name with its
  own name as a string value, followed by one `Object.freeze(<operation-enum-name>)`. A file without a
  syntax tree is left to `typescript/parse`. Reported at the file.
- `#marker`: checked only against a valid manifest, in every tracked file below `layout.source-root`
  whose name ends with `.ts`, declaration files included, in every folder: this walk skips none of
  `[languages.typescript] skip-folders`. A line holding
  `marker-token` must hold `<marker-token> <FAMILY>.<RESOURCE>=<family id>,<resource id>` naming a
  static resource with its identifiers (reported at the line). When every marker is right: a marker
  listed in `marker-paths` must stand in exactly the listed files (compared with the sorted files found,
  so the list must be sorted; reported at the first file), an unlisted marker in at most one file
  (reported at the second), and every `required-markers` entry must be present (reported at the
  manifest).

Two identical findings are reported once.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `manifest`, `marker`,
`operation-enum`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `manifest-path` | path | `layout.source-root` + `/common/advisory-lock/registry.manifest.json` | The registry manifest. |
| `operation-enum-path` | path | `layout.source-root` + `/common/advisory-lock/enum/operation.enum.ts` | The operation enum's file. |
| `operation-enum-name` | string | `"EDatabaseAdvisoryLockOperation"` | The operation enum's name. |
| `operations` | list of strings, at least one | the ten `SESSION_*` and `TRANSACTION_*` lock, try-lock and unlock operations | The operations, in enum order. |
| `marker-token` | string | `"DATABASE_ADVISORY_LOCK_STATIC_KEY"` | The token that starts a static-key marker. |
| `marker-paths` | map `FAMILY.RESOURCE` → sorted list of paths | `{}` | The exact files of a marker. |
| `required-markers` | list of `FAMILY.RESOURCE` | `[]` | The markers that must be present. |

```toml
[rules]
"nestjs/advisory-lock-manifest" = { level = "error", options = { required-markers = ["STOCK.REBUILD"] } }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/advisory-lock-manifest#marker src/stock/rebuild.ts:1
  Static-key marker STOCK.REBUILD does not match the manifest.
```

## How to fix it

Correct the manifest and regenerate the registry from it; keep the operation enum exact; write each
marker with the identifiers the manifest gives its resource, in the files `marker-paths` lists.

# `nestjs/adapter-boundary`

Only infrastructure loads external adapters, and inward owners load no infrastructure.

## What it checks and why

Drivers and cloud clients belong behind adapters in the infrastructure folder, so the rest of the code
depends on contracts it owns and can be tested and replaced without the driver. An inward owner that
loads infrastructure directly binds itself to one adapter.

Every `.ts` file below `layout.source-root` (outside `[languages.typescript] skip-folders`, declaration files included)
is read for the modules it loads: imports, re-exports, type-only imports, `require`, `import()` and
`import()` types.

- `#external-package`: a file outside `infrastructure-prefix` loads a specifier that equals one of
  `external-packages` or is a subpath of one, or starts with one of `external-package-prefixes`.
- `#inward-infrastructure`: a file whose path starts with one of `inward-owner-prefixes` loads the
  infrastructure alias or a path below it, or a relative specifier that lands in `infrastructure-prefix`.
  The alias is `@` and the prefix without the leading source root and the trailing `/`
  (`src/infrastructure/` gives `@infrastructure`). A specifier spelled from the project root is not
  checked here.

Each finding is reported once per file and specifier, at the file.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `external-package`,
`inward-infrastructure`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `external-packages` | list of strings | `[]` | External adapter packages. |
| `external-package-prefixes` | list of strings | `[]` | Specifier prefixes of external adapter packages. |
| `infrastructure-prefix` | string | `layout.infrastructure-folder` and `/` | The path prefix of the infrastructure folder. |
| `inward-owner-prefixes` | list of strings | `[]` | Path prefixes of the owners that load no infrastructure. |

```toml
[rules."nestjs/adapter-boundary"]
level = "error"
options = { external-packages = ["pg", "redis"], external-package-prefixes = ["@aws-sdk/"], inward-owner-prefixes = ["src/modules/"] }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/adapter-boundary#external-package src/modules/user/user.service.ts
  src/modules/user/user.service.ts imports external adapter package pg outside infrastructure
```

## How to fix it

Wrap the package in an adapter below the infrastructure folder; have the inward owner declare the
contract it needs and bind the adapter to it where the application is composed.

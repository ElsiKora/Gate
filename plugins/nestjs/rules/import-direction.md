# `nestjs/import-direction`

Source directories load only what their dependency contracts allow.

## What it checks and why

The direction of dependencies is what keeps planes and owners replaceable: the domain does not know its
transports, shared code does not know features, and migrations do not import the code they outlive. A
dependency contract writes one such direction down for a folder, and the rule holds every file below it
to the contract.

For each contract, every `.ts` file below `source-directory` (outside `[languages.typescript] skip-folders`) is read
for the modules it loads: imports, re-exports, `require`, `import()` and `import()` types. A load is
reported when the contract forbids it:

- it names a source path, through a relative specifier, a path below `layout.source-root` or a
  `layout.path-aliases` alias, that equals one of `forbidden-owner-paths` or lies below one, or that
  names one of `forbidden-source-paths` (script extensions are ignored, and a folder names its
  `index` file);
- or the specifier equals one of `forbidden-packages` or is a subpath of one, or starts with one of
  `forbidden-package-prefixes`.

An edge in `allowed-dependencies` wins over every forbidden match. Files in `allowed-source-paths` or
below `allowed-source-directories` are not bound. Each contract reports on its own, so one load can be
reported by several contracts. Reported at the file, under the contract's `aspect`.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `domain-direction`,
`migration-direction`, `owner-contract`, `plane-direction`, `plane-migration`, `shared-purity`. Findings
carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `contracts` | list of contracts, at least one | required | The dependency contracts. Without one the options are an error; leave the rule off instead. |

A contract:

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `source-directory` | path | required | The folder whose files the contract binds. |
| `aspect` | aspect name | `"owner-contract"` | The aspect its findings carry. |
| `forbidden-owner-paths` | list of paths | required | Source folders and files the files may not load. |
| `forbidden-packages` | list of strings | required | Packages the files may not load. |
| `forbidden-package-prefixes` | list of strings | `[]` | Specifier prefixes the files may not load. |
| `forbidden-source-paths` | list of paths | `[]` | Source files the files may not load. |
| `runtime-values-only` | boolean | `false` | Count only loads of runtime values, not type-only ones. |
| `include-declaration-files` | boolean | `true` | Bind declaration files too. |
| `allowed-source-paths` | list of paths | `[]` | Files the contract does not bind. |
| `allowed-source-directories` | list of paths | `[]` | Folders whose files the contract does not bind; `""` is the project root. |
| `allowed-dependencies` | list of `{ source-path, target-paths }` | `[]` | Source files one file may load despite the contract. |

```toml
[[rules."nestjs/import-direction".options.contracts]]
source-directory = "src/modules/domain"
aspect = "domain-direction"
forbidden-owner-paths = ["src/modules/public"]
forbidden-packages = ["@nestjs/swagger"]
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/import-direction#domain-direction src/modules/domain/user/user.service.ts
  src/modules/domain/user/user.service.ts loads @modules/public/user outside its src/modules/domain dependency contract
```

## How to fix it

Load the module through an owner the contract allows, invert the dependency behind a contract the
importing owner declares, or move the code to the owner it depends on.

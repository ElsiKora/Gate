# `nestjs/raw-config-access`

Only approved configuration boundaries reach a raw configuration service.

## What it checks and why

Feature code reads typed runtime configuration. A file that reaches the raw configuration service reads
keys by string, without the types, defaults and validation of the configuration boundary, and the key
inventory no longer lists every reader.

A `.ts` file below `layout.source-root` is reported when any name, export or import type in it reaches a
raw configuration export: one of the `names` of a module `raw-imports` lists. Names are followed across
the source files as the TypeScript checker follows them, without loading any package: named imports and
re-exports, `export *` and `export * as` edges, namespaces, `const` aliases, destructuring, `require`,
`import(...)` types and `await import`. Among a module's exports, the first declaration of a name wins.
A module matches by exact specifier, so a subpath of a listed package does not match. Relative and
aliased specifiers resolve with the `[languages.typescript]` settings.

A file is inside the boundary, and never reported, when its path equals one of `approved-files`, starts
with one of `approved-prefixes`, or contains one of `approved-segments`. Declaration files are included;
`[languages.typescript] skip-folders` are skipped. Reported at the file.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `raw-imports` | list of `{ module, names }`, at least one | required | The raw configuration services. Without one the options are an error; leave the rule off instead. |
| `approved-files` | list of paths | `[]` | Files of the configuration boundary. |
| `approved-prefixes` | list of strings | `[]` | Path prefixes of the configuration boundary. |
| `approved-segments` | list of strings | `[]` | Path fragments of the configuration boundary, matched anywhere. |

```toml
[rules."nestjs/raw-config-access"]
level = "error"
options = { raw-imports = [{ module = "@nestjs/config", names = ["ConfigService"] }], approved-prefixes = ["src/config/"] }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/raw-config-access src/modules/user/user.service.ts
  src/modules/user/user.service.ts imports a raw config service outside an approved config boundary
```

## How to fix it

Read the value through the typed runtime configuration, or, when the file is part of the configuration
boundary, move it there.

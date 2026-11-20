# `nestjs/error-string-shim`

No source enum copies an installed `ErrorString` action enum.

## What it checks and why

NestJS-Crud-Automator installs the `ErrorString` action enums. A source enum of the same name shadows
the installed one with a local copy: the copy misses the actions a later release adds, and an error
built from it no longer matches the automator's own.

Every enum declared in a `.ts` file (not `.d.ts`) below `layout.source-root`, outside
`[languages.typescript] skip-folders`, whose name is one of `enum-names` is reported at its name.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. The finding's key is the
enum's name.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `enum-names` | list of strings | `["EErrorStringAction", "EErrorStringCompositeAction"]` | The installed enums no source enum may copy. |

```toml
[rules]
"nestjs/error-string-shim" = "error"
```

## Example finding

```text
error nestjs/error-string-shim src/modules/user/user.enum.ts:2
  src/modules/user/user.enum.ts declares EErrorStringAction, a local copy of the installed Crud Automator enum; use its installed members and record a missing action as a follow-up
```

## How to fix it

Delete the local enum and use the installed enum's members; record an action the installed enum lacks
as a follow-up for the automator.

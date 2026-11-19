# `nestjs/module-names`

A module class names its resource folder, and a semantic owner's classes carry the owner's full
stem.

## What it checks and why

The module class and the folder it lives in name the same resource, so either one finds the other:
`src/modules/domain/book/loan/module.ts` exports `BookLoanModule`. A class named after another
resource, or a folder that drifted from its class, sends readers and imports to the wrong place.

Each tracked `module-file`, in every folder (the walk skips none of `[languages.typescript] skip-folders`),
that lies below one of `layer-scopes` (the first scope whose `path`
starts the file's path wins) is judged by the first top-level `export class` or
`export default class` it declares; an anonymous class counts as none. The checks run in order and
stop at the first finding:

1. **`module-class`.** The file exports a named class.
2. **`module-owner-prefix`.** The class starts with the scope's `class-prefix`. In a scope without
   one, a `resource-class-prefixes` entry whose `resource` is the resource folder below the scope,
   or a folder above it, names the prefix instead.
3. **`module-class`.** The class ends with `module-suffix`.
4. **`semantic-owner-class`.** For a folder of `semantic-owners`, the class is the owner stem
   followed by `module-suffix`. This replaces the folder check.
5. **`resource-folder`.** The class stem, without the suffix and the scope's class prefix, in kebab
   case spells the resource folder path below the scope with `/` written as `-`. Kebab case puts a
   dash between a lowercase letter or digit and a capital, and before the last capital of a run of
   capitals that starts a word (`HTTPServer` → `http-server`), lowercases everything, then applies
   `token-replacements` left to right to whole dash-separated tokens.

Each file of `semantic-class-files` that exists in a semantic owner's folder exports the owner stem
followed by the file's `suffix` (`semantic-owner-class`). Each `stale-module-paths` entry that is a
tracked file, or a folder with tracked files, is reported (`shortened-resource-folder`). A file
without a syntax tree is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `module-class`,
`module-owner-prefix`, `resource-folder`, `semantic-owner-class`, `shortened-resource-folder`.
Findings are located at the file or folder they name and carry no target.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `layer-scopes` | array of {`path`, `class-prefix`} | `src/modules/domain/` → `""`, `src/modules/internal/` → `"Internal"`, `src/modules/integration/` → `"Integration"`, `src/modules/public/` → `"Public"`, `src/infrastructure/` → `"Infrastructure"` | The scopes whose module files are judged and the class prefix of each. |
| `resource-class-prefixes` | array of {`resource`, `class-prefix`} | `[]` | Class prefixes of resources in scopes without a class prefix. |
| `module-file` | string | `"module.ts"` | The name of a module file. |
| `module-suffix` | string | `"Module"` | The suffix of a module class. |
| `semantic-class-files` | array of {`file`, `suffix`} | `controller.ts` → `"Controller"`, `entity/entity.ts` → `"Entity"`, `service.ts` → `"Service"` | The class files of a semantic owner besides its module file. |
| `semantic-owners` | array of {`resource-path`, `owner-stem`} | `[]` | Resource folders whose classes carry `owner-stem` instead of the folder's stem. |
| `stale-module-paths` | array of string | `[]` | Shortened module paths that must not exist; `""` is refused. |
| `token-replacements` | array of [string, string] | `[]` | Whole-token replacements applied to the expected folder name. |

The rule reads no field of the layout block: its scopes are its own.

```toml
[rules]
"nestjs/module-names" = { level = "error", options = { resource-class-prefixes = [{ resource = "staff", class-prefix = "Staff" }], token-replacements = [["read-ease", "readease"]] } }
```

## Example finding

```text
error nestjs/module-names#resource-folder src/modules/domain/book/module.ts
  src/modules/domain/book/module.ts is under book; ShelfModule should live under shelf
```

## How to fix it

Rename the class after its folder, or move the module to the folder its class names. For a semantic
owner, use the owner stem on the module and on each class file. Rename a stale shortened folder to
the full name of its entity resource.

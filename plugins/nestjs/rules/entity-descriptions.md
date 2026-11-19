# `nestjs/entity-descriptions`

The API descriptions of entity properties are static, start in lowercase and do not repeat their
owner's name.

## What it checks and why

The CRUD automator builds the `OpenAPI` documentation of an entity from the description each property
carries. A description computed at run time is not documented; one that repeats the owner's name
("book title" on `BookEntity.title`) reads twice in every generated schema; an uppercase start breaks
the house style unless the word is an accepted acronym.

The rule reads every file with one of `extensions` at the project root or below one of `roots`,
outside `[languages.typescript] skip-folders`, that names `framework-package`. In every class (at any
depth), each member decorator call (`@X(...)` or `@X?.(...)`) that resolves to `describe-decorator` of
the package, with an object literal as first argument, has its first plain `description-key`
property read:

- `#static`: the value is neither a string literal nor a constant the rule can read across files.
- `#owner-prefix`: the description starts with a trailing run of the owner's name tokens, the owner's
  terminal `owner-suffix` left out. `LibraryShelfEntity` has the tokens `library shelf`; "shelf
  height" and "library shelf code" repeat it, "height" does not.
- `#uppercase-token`: the description's first word (hyphenated parts included, as `SHA-256`) starts
  with an uppercase letter and is not in `allowed-initial-uppercase`.

Names split into tokens at case changes and digits: `HTTPServer2Error` has `http server 2 error`.

Scope: project. Language: `typescript`. Reads history: no.

**Targets.** `#owner-prefix` and `#uppercase-token` findings carry the target
`<Class>.<member>: <description>`, without a path, so one exception covers the description wherever
it is declared. A member name prints as the compiler prints it: `#name`, a string key's value, a
numeric key's value, a computed key with its brackets.

```toml
[[exception]]
rule = "nestjs/entity-descriptions#uppercase-token"
target = "BookEntity.title: Publisher-authored title"
reason = "the title names its author in its first word"
```

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.
`roots` are prefixes matched against paths as text, so they keep a trailing `/` when one is
written: `src/` matches `src/book.ts` and not `srcgen/book.ts`.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `roots` | array of string | `["infrastructure/", "scripts/", "src/", "test/"]` | Path prefixes read, besides the project's root files. |
| `extensions` | array of string | `["ts", "tsx"]` | The extensions of the files read. |
| `framework-package` | string | absent: the layout's package (`layout.automator-package`) | The package of the describe decorator. |
| `describe-decorator` | string | `"ApiPropertyDescribe"` | The decorator that carries the description. |
| `description-key` | string | `"description"` | The option that holds the description. |
| `owner-suffix` | string | `"Entity"` | A class name suffix that is not one of the owner's tokens. |
| `allowed-initial-uppercase` | array of string | `[]` | Uppercase first words that are allowed, such as acronyms. |

```toml
[rules]
"nestjs/entity-descriptions" = { level = "error", options = { allowed-initial-uppercase = ["IP", "TLS"] } }
```

## Example finding

```text
error nestjs/entity-descriptions#owner-prefix src/modules/shelf/shelf.entity.ts:3
  LibraryShelfEntity.height description repeats its owner/resource prefix: "shelf height"
```

## How to fix it

Write the description as a string literal or a string constant, start it in lowercase, and describe
the property without the owner's name ("height", not "shelf height"). Add a real acronym to
`allowed-initial-uppercase`; record a deliberate exception in `gate.exceptions.toml` with its target.

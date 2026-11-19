# `nestjs/dto-decorators`

DTOs use the CRUD automator's property decorators, and its describe decorator stays in entity folders.

## What it checks and why

The automator generates a DTO's `OpenAPI` metadata from the entity's property descriptions. A DTO that
adds the `OpenAPI` package's own `ApiProperty*` decorators describes the same property twice, and the two
drift; a description written outside the entity is not the one the automator reads.

- `#swagger-decorator`: a `.ts` file below `layout.modules-root` whose path contains `/<dto-folder>/` and
  whose text both imports from `layout.swagger-package` (`from`, at least one whitespace character, the
  package name in quotes) and contains the word `swagger-property-word` not preceded by a letter, digit
  or `_`. The test is on raw text, read lossily, so it does not depend on the file parsing. Reported at
  the file.
- `#describe-outside-entity`: a `.ts` file below `layout.source-root` whose path does not contain
  `/<entity-folder>/` and that decorates a class, property, accessor, method or parameter with
  `describe-decorator`, imported by name (aliases included) or through a namespace import of
  `layout.automator-package`. One finding per file, at one decorator: an outer decorated node before the
  nodes it contains, and of two unrelated ones the later.

Declaration files are included in both walks; `[languages.typescript] skip-folders` are skipped.

The `#describe-outside-entity` walk skips a file that cannot be parsed; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `describe-outside-entity`,
`swagger-decorator`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `dto-folder` | name | `"dto"` | The folder that marks a DTO file. |
| `entity-folder` | name | `"entity"` | The folder that marks an entity file. |
| `describe-decorator` | string | `"ApiPropertyDescribe"` | The automator's describe decorator. |
| `swagger-property-word` | string | `"ApiProperty"` | The word that names the `OpenAPI` package's property decorators. |

```toml
[rules]
"nestjs/dto-decorators" = { level = "error" }
```

## Example finding

```text
error nestjs/dto-decorators#swagger-decorator src/modules/user/dto/create.dto.ts
  src/modules/user/dto/create.dto.ts imports Nest Swagger ApiProperty decorators in a DTO folder
```

## How to fix it

Use the automator's property decorators in DTOs, and describe each property once, on its entity.

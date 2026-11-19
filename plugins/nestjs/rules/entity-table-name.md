# `nestjs/entity-table-name`

Every `TypeORM` entity decorator names its table explicitly, with a double-quoted string as its first
argument.

## What it checks and why

Without an explicit name, `TypeORM` derives the table name from the class name, so renaming a class
silently renames its table and the next migration drops and recreates it. A double-quoted literal keeps
the table name in one searchable place.

Every tracked `.ts` file below `layout.source-root` is read as text (invalid UTF-8 bytes are replaced),
declaration files included, in every folder: the walk skips none of `[languages.typescript] skip-folders`. The scan is textual, as the regular
expression `/@Entity\s*\(([^)]*)\)/g` reads a file:

- `decorator` followed by optional JavaScript white space and `(` starts a match; the arguments end
  at the first `)`, and the search resumes after it;
- the match passes when its arguments, trimmed, are a non-empty double-quoted string, optionally
  followed by a comma and further arguments;
- every other match is a finding: `()`, a single-quoted or template string, an options object, `""`;
- comments and strings count, and the import of the decorator is not checked.

Each finding names the column (one-based, in characters) of the match, so two matches on one line
are two findings.

Scope: project. Language: `typescript`. Reads history: no. One aspect (`""`). Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `decorator` | string | `"@Entity"` | The decorator text, `@` included, whose first argument must name the table; it must not be empty. |

The files read come from `[plugins.nestjs] options` `layout.source-root`.

```toml
[rules]
"nestjs/entity-table-name" = { level = "error", options = { decorator = "@Entity" } }
```

## Example finding

```text
error nestjs/entity-table-name src/modules/book/book.entity.ts:3
  src/modules/book/book.entity.ts uses @Entity() at column 1 without an explicit double-quoted table name; use @Entity("table_name")
```

## How to fix it

Name the table first, in double quotes: `@Entity("book")`, or `@Entity("book", { schema: "library" })`
when the entity has further options.

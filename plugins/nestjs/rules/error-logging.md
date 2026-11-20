# `nestjs/error-logging`

Native error logs carry formatted evidence, and native `TypeORM` logging stays disabled.

## What it checks and why

An error object passed raw to a log carries whatever it holds: request bodies, connection strings,
tokens. The CRUD automator's evidence formatter keeps only what is safe to log, so every native error log
goes through it, with a message that is bounded rather than built from the error. The ORM's own query
logging prints parameters, so it stays off: its options end with `logging: false`, and nothing changes
that later.

Every `.ts` file (not `.d.ts`) below `roots`, in every folder (the walk skips none of
`[languages.typescript] skip-folders`), is read, each on its own as a
compiler program of that one file reads it: names, the members of `this` and of the file's classes, and
imports matched by their literal module specifier, without resolution.

- `#logger-error-evidence`: a call of `logger-method` on `logger-class` of `layout.nest-common-package`,
  or on an instance of it (`.bind`, `.call` and `.apply` followed), that does not pass exactly two
  arguments in this order: `(evidence, message)` for a call on the class, `(message, evidence)` for
  a call on an instance. The evidence is `evidence-formatter` of `layout.automator-package` called
  with one argument that is error evidence, or a `const` alias of that call; the message is
  bounded. Reported at the call.
- `#typeorm-logging-disabled`: native options, a value typed with one of `orm-options-types` (through a
  local alias or one of `wrapper-types`), the first argument of `orm-constructor` of
  `layout.typeorm-package`, or the return of a function declared to return them, that:
  - is not a local object literal on every branch (reported at the value);
  - does not end with a `logging-property` set to the literal `false`, where a later spread or computed
    key undoes it (reported at the object);
  - or has that property written later, directly or through a local alias, including by `Object.assign`,
    `Object.defineProperty`, `Reflect.set`, `Reflect.deleteProperty` and `Reflect.defineProperty`
    (reported at the write).

Error evidence is the name a `catch` clause binds, at any depth; a parameter typed `unknown` or the
global `Error`, a callback's parameter included; or a `const` alias of one of these. An untyped
parameter is not error evidence, so a rejection callback's parameter counts only when it is typed.

A bounded message is a string literal, any template literal, a conditional of two bounded messages,
a parameter typed `string`, or a `const` alias of one of these, as long as it reads no raw evidence.
Raw evidence is error evidence, a name a `catch` clause binds by destructuring, and any variable,
destructured binding or member of one of the file's classes whose initializer reads raw evidence;
reading is followed through those initializers. So `` `Job ${jobId}: drain failed` `` is bounded
when `jobId` is a `string` parameter or a `const` whose value reads no raw evidence, and is not when
`jobId` is read from the caught error.

Messages start with the file and line. Two identical findings are reported once.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `logger-error-evidence`,
`typeorm-logging-disabled`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `roots` | list of paths | `layout.source-root` | The folders of production code. |
| `logger-class` | string | `"Logger"` | The native logger class. |
| `logger-method` | string | `"error"` | The logger's error method. |
| `evidence-formatter` | string | `"FormatErrorEvidenceForLog"` | The evidence formatter. |
| `orm-options-types` | list of `{ module, name }` | `DataSourceOptions` of `typeorm`, and `PostgresDataSourceOptions` of `typeorm/driver/postgres/PostgresDataSourceOptions` with and without `.js` | The types of native ORM options. |
| `orm-constructor` | string | `"DataSource"` | The class whose first argument is native options. |
| `logging-property` | string | `"logging"` | The property that must end as `false`. |
| `wrapper-types` | list of strings | `["Promise", "Readonly", "Awaited", "NonNullable", "Extract"]` | Library generics whose first argument is the type they wrap. |

```toml
[rules]
"nestjs/error-logging" = { level = "error" }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/error-logging#logger-error-evidence src/modules/user/user.service.ts:7
  src/modules/user/user.service.ts:7 native Logger.error must pass FormatErrorEvidenceForLog(actual error evidence) and a bounded message/context, with no raw additional arguments
```

## How to fix it

Log `FormatErrorEvidenceForLog(error)` with a bounded message and nothing else, in the order the
call takes. A rejection callback types its parameter `unknown`:

```ts
import { Logger } from "@nestjs/common";
import { FormatErrorEvidenceForLog } from "@elsikora/nestjs-crud-automator";

export class JobRunner {
  private readonly logger = new Logger(JobRunner.name);

  drain(jobId: string, work: Promise<void>): void {
    work.then(undefined, (reason: unknown) => {
      this.logger.error(`Job ${jobId}: drain failed`, FormatErrorEvidenceForLog(reason));
    });
  }

  stop(jobId: string): void {
    try {
      this.halt();
    } catch (error) {
      Logger.error(FormatErrorEvidenceForLog(error), `Job ${jobId}: stop failed`);
    }
  }

  private halt(): void {}
}
```

With an untyped `(reason) =>`, the first call is reported.

Write the ORM options as an object literal that ends with `logging: false`, and do not change them
afterwards.

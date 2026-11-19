# `nestjs/native-repository`

A native service's constructor-injected repository is a `protected readonly` parameter property.

## What it checks and why

A native service of the CRUD automator reads its repository through the inherited base class. A
repository parameter property that is `private`, `public` or writable hides it from the base class or
lets a caller replace it.

A top-level class of a `.ts` file below `layout.source-root` (declaration files and the folders of
`[languages.typescript] skip-folders` left out) is a native service when:

- it carries a call of `service-decorator` imported from `framework-package` (directly or through a
  namespace import);
- it extends `service-base` of `framework-package`; or
- it extends a top-level class that is a native service, followed across files through the project's
  module resolution (`[languages.typescript] tsconfig`).

Every constructor parameter named `repository-parameter` that is a parameter property (it carries an
accessibility, `readonly` or `override`) must be both `protected` and `readonly`. A plain argument
forwarded to the base class is no parameter property and passes. The finding is at the parameter's
first decorator, or at the parameter when it has none.

Scope: project. Language: `typescript`. Reads history: no. One aspect (`""`). Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `framework-package` | string | absent: the layout's package (`layout.automator-package`) | The package of the service decorator and base class. |
| `service-decorator` | string | `"ApiService"` | The class decorator that makes a native service. |
| `service-base` | string | `"ApiServiceBase"` | The base class that makes a native service. |
| `repository-parameter` | string | `"repository"` | The constructor parameter that holds the repository. |

```toml
[rules]
"nestjs/native-repository" = { level = "error", options = { repository-parameter = "repository" } }
```

## Example finding

```text
error nestjs/native-repository src/modules/book/book.service.ts:5
  src/modules/book/book.service.ts:5 BookService.repository must be a protected readonly native repository parameter property
```

## How to fix it

Declare the parameter as `protected readonly repository: Repository<Entity>`.

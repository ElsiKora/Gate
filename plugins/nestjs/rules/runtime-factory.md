# `nestjs/runtime-factory`

Runtime start files construct Nest only through their role's `NestFactory` methods.

## What it checks and why

A service that runs in several roles (an API, a worker, a one-shot job) starts each role from its own
file, and each role builds its Nest application in one way: an HTTP application, an application context,
a microservice. Building it another way starts the wrong runtime. For each contract the rule reads the
start file and reports:

- a missing start file;
- each listed method the file never calls (a method listed twice is reported once);
- each other method of the factory class the file calls, at the line of its first call.

Calls count through named, aliased and namespace imports of `layout.nest-core-package`, through `const`
aliases, and as optional calls; a local name that shadows the import does not count. Files without a
contract are never read, and a file that cannot be parsed is left to `typescript/parse`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `contracts` | array of `{ file, methods }`, at least one | required | Each start file and the factory methods it must call, which are also the only ones it may call. Without one the options are an error; leave the rule off instead. |
| `factory-class` | string | `"NestFactory"` | The factory class exported by `layout.nest-core-package`. |

```toml
[rules]
"nestjs/runtime-factory" = { level = "error", options = { contracts = [
  { file = "src/runtime/api/function/start.function.ts", methods = ["create"] },
  { file = "src/runtime/worker/function/start.function.ts", methods = ["createApplicationContext"] },
] } }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/runtime-factory src/runtime/api/function/start.function.ts:3
  src/runtime/api/function/start.function.ts calls native NestFactory.createMicroservice outside its role contract
  help: use only the role's NestFactory methods, or add createMicroservice to its contract
```

## How to fix it

Construct the application with the methods the role's contract lists. If the role really changed, change
its contract in the configuration.

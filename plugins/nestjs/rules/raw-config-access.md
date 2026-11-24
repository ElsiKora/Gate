# `nestjs/raw-config-access`

Only approved configuration boundaries reach a raw configuration service or `process.env`, and the
example environment file lists exactly the keys the code reads.

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
`[languages.typescript] skip-folders` are skipped. Reported at the file, under `boundary`.

The same files outside the boundary may not read `process.env`, the global (a local name `process` is
not it): a member (`process.env.KEY`), an element (`process.env["KEY"]`), a destructuring
(`const { KEY } = process.env`), an `in` test, or `process.env` itself. Reported at each read, under
`process-env`.

**Coverage.** A key is read when a getter of a raw configuration service (`getters`, called on a name
or class property typed as one of the services, or on a service itself) or a read of `process.env`
names it, in any file below the source root, inside the boundary or not. The key is named when it is
a string literal, a template without substitutions, a `const` bound to one, the variable of a
`for ... of` loop over a literal array of strings, or the element parameter of a callback over one,
in the same file. A literal array of strings is written in place or is a `const` bound to one, also
under `as`, `satisfies` and `!`. The element parameter is the first parameter, a plain name (not
destructured, without a default, not a rest parameter), of an arrow function or function expression
passed directly as the first argument of `map`, `flatMap`, `forEach`, `filter`, `find`, `findIndex`,
`findLast`, `findLastIndex`, `some` or `every` called on the array, with or without optional
chaining: in
`Object.fromEntries(["A", "B"].map((key) => [key, config.getOrThrow(key)]))` the getter reads `A` and
`B`. The first parameter of a `reduce` or `reduceRight` callback is the accumulator, and a callback
passed by name, a later parameter, or a method called on anything else names no key.
A loop variable or callback parameter that anything assigns, updates or redeclares names no key.
An empty array names no key, so a read through its loop variable or callback parameter is `dynamic`.

A getter call whose first argument can never be a key is no read and is reported under no coverage
aspect: an object or array literal, an arrow function or function expression, a class expression,
or a numeric, bigint, boolean, `null` or regular-expression literal, also under parentheses, `as`,
`satisfies` and `!`. So `store.get({ name, section })` on a raw service that is also a store is not a
read. This applies to getters only: a `process.env` element with such a key is still `dynamic`.

The reads are compared with `example-file`, whose lines are `KEY=value`, `export KEY=value`,
comments and blank lines:

| Aspect | Reported when | At |
| --- | --- | --- |
| `undeclared` | a key the code reads has no line in `example-file` | the first read of each file that reads it |
| `unread` | a line names a key nothing reads, and `external-keys` does not list it | the line |
| `dynamic` | a read names its key some other way: a name, call, member or other expression whose value is not known, such as a parameter | the read |
| `example-missing` | `example-file` is not tracked and the code reads a key | `example-file`, once |
| `example-syntax` | a line of `example-file` is no assignment, comment or blank line | the line |

No finding holds a value of `example-file`: the rule reads its keys only. The comparison is
`gate_typescript::env`, the module every rule about environment coverage calls.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `boundary`, `process-env`,
`undeclared`, `unread`, `dynamic`, `example-missing`, `example-syntax`. Targets: `<path>:<line>` for
`process-env` and `dynamic`, `<path>:<key>` for `undeclared` and `unread`, the example's path for
`example-missing`, `<example>:<line>` for `example-syntax`; a `boundary` finding has none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `raw-imports` | list of `{ module, names }`, at least one | required | The raw configuration services. Without one the options are an error; leave the rule off instead. |
| `approved-files` | list of paths | `[]` | Files of the configuration boundary. |
| `approved-prefixes` | list of strings | `[]` | Path prefixes of the configuration boundary. |
| `approved-segments` | list of strings | `[]` | Path fragments of the configuration boundary, matched anywhere. |
| `example-file` | path | `".env.example"` | The example environment file. |
| `getters` | list of strings | `["get", "getOrThrow"]` | The methods of a raw configuration service whose first argument is a key. |
| `external-keys` | list of `{ key, reader }` | `[]` | Keys something other than the project's TypeScript reads (Node.js itself, a local script, the Dockerfile), each with what reads it; they count as read. |

```toml
[rules."nestjs/raw-config-access"]
level = "error"
options = { raw-imports = [{ module = "@nestjs/config", names = ["ConfigService"] }], approved-prefixes = ["src/config/"], external-keys = [{ key = "NODE_OPTIONS", reader = "Node.js" }] }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/raw-config-access#boundary src/modules/user/user.service.ts
  src/modules/user/user.service.ts imports a raw config service outside an approved config boundary
error nestjs/raw-config-access#undeclared src/config/app.config.ts:12
  API_PORT is read here but .env.example has no line for it
```

## How to fix it

Read the value through the typed runtime configuration, or, when the file is part of the configuration
boundary, move it there. Add a `KEY=` line with a placeholder value for each key the code reads, and
remove the lines nothing reads; a key that only the runtime or a script reads goes in `external-keys`
with what reads it. Name a key with a literal where the code computes it.

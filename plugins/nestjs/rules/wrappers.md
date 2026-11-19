# `nestjs/wrappers`

No function, method or field only forwards its parameters to one call or only throws or returns one
error construction, and no Nest exception is constructed without a message of the message factory.

## What it checks and why

A wrapper that only forwards to another call, or only builds one error, adds a name without adding
behaviour: readers follow one more hop, and the error's message and status are decided away from
the place that knows why it fails. A Nest exception built without the factory's message bypasses
the service's error catalogue.

The rule reads every `.ts` file below `layout.source-root` (declaration files left out), in every folder:
the walk skips none of `[languages.typescript] skip-folders`. A file without a syntax tree is skipped; `typescript/parse` reports it. Values are
followed across the `.ts` files below the source root through `const` bindings, top-level object
destructuring, property reads with literal names, imports, named, default, namespace and star
re-exports, with module specifiers resolved through `[languages.typescript] tsconfig`. An import of
a known package (see `package-exports-file`) resolves through that package's export list.

The members read are methods with a body, object literal methods, function declarations, variables
initialized with a function or arrow function, and class fields and accessors initialized with one.
A member is a framework boundary when it has a decorator, is one of `lifecycle-methods`, is a
reporter adapter's method (`reporter-adapters`), or is a framework method of its folder
(`framework-methods`).

Scope: project. Language: `typescript`. Reads history: no.

### `one-call-forwarding`

The body is one call (returned, awaited or as a statement) that passes the member's parameters on
unchanged: each argument is a parameter, a property read from one, a `this` property that is not a
`readonly` field with a fixed value, or an array or object literal of those. The callee is a name or
a property chain from a name, `this` or `super` that is not a parameter. A member with a parameter
default, or a framework boundary, is not reported.

### `intrinsic-alias`

The same, when the callee is one of `intrinsic-calls`, such as `JSON.stringify(value)`. Such a call
of a new array or object literal builds a value and is not reported.

### `throw-only-error`

The body only throws or returns one error construction, after `const` bindings that only feed it.
An error construction is a Nest exception or a plain error (`plain-errors`, when no local binding
shadows the name). At a framework boundary a body that throws is allowed; one that returns is not.
An exhaustiveness guard, which returns `never` and takes a required parameter of type `never`
(also through local type aliases), is allowed. A private one-parameter method whose only statement
is `if (x)` or `if (!x)` without `else`, and which every use in its class calls with the one boolean
literal that enters the branch, is read through its branch. A Nest exception construction that an
exception of `raw-nest-exception` names is no error construction here.

### `raw-nest-exception`

A Nest exception is an export of one of `exception-packages` whose name ends with
`exception-suffix`, however the file binds it (alias, namespace, re-export). Its first argument
must be a call of the message factory, directly or as a field of a frozen object literal
(`Object.freeze({ ... })`) reached through constants.

The finding's target names the construction for exceptions:

```text
<path>: <scope>[ catch][ parameters <name>[, <name>]]: <construction>
```

- `<scope>` is `<Class>.<member>` for a construction in a class member, or `<function>` for one in
  a named function declared at the top level of the file, when no class of the file has that name.
- `catch` is present when the construction lies in a `catch` clause of that scope.
- `parameters` lists, in declaration order, the plain parameters (no default, no destructuring,
  never reassigned) that the construction's arguments read, of the scope's own function and of the
  innermost callback holding the construction, such as the `baseUrl` of
  `findListed(baseUrl: string)` or the `errors` of a `ValidationPipe` `exceptionFactory`:
  `src/x.ts: findListed parameters baseUrl: new BadRequestException({ baseUrl })`.
- `<construction>` is the construction in canonical form, with the exception's export name as its
  constructor: `new BadRequestException({ code: "INVALID" })`. When the construction is the first
  argument of a call whose callee reads no local binding, the target names the call,
  `super.handleError(new IntrinsicException())`; an exception that names only the construction
  still matches it there.

The canonical form prints the syntax tree on one line: one space after each comma and colon and
around operators, `{ a: 1 }` and `[a, b]`, strings in double quotes, numbers as JavaScript prints
them, every parameter list in parentheses, and function bodies by their source text without
comments, white space collapsed. White space, comments, trailing commas, quotes and number
spellings do not change the target; parentheses and the spelling of an object key (quoted,
shorthand or computed) do.

A construction outside such a scope, or whose arguments read another binding local to the scope
(a local variable, a parameter with a default or reassigned, `arguments`), has the target
`<path>:<line>: <construction>`.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `exception-packages` | array of string | `["@nestjs/common", "@nestjs/throttler"]` | The packages whose exceptions are Nest exceptions. |
| `exception-suffix` | string | `"Exception"` | The suffix of a Nest exception's export name. |
| `message-factory` | `{ package, name }` | `{ name = "ErrorString" }`, `package` absent: the layout's package (`layout.automator-package`) | The message factory. The name appears in the message. |
| `plain-errors` | array of string | `["AggregateError", "Error", "EvalError", "RangeError", "ReferenceError", "SyntaxError", "TypeError", "URIError"]` | The built-in errors. |
| `intrinsic-calls` | array of `[object, method]` | `[["JSON", "stringify"], ["Object", "freeze"]]` | The built-in calls of `intrinsic-alias`. |
| `lifecycle-methods` | array of string | `["beforeApplicationShutdown", "onApplicationBootstrap", "onApplicationShutdown", "onModuleDestroy", "onModuleInit"]` | Boundary methods in every class. |
| `framework-methods` | table of path segment to array of string | see below | Boundary methods in files whose path contains the segment. |
| `reporter-adapters` | array of `{ module, port, method, path-prefix }` | `[]` | A class below `path-prefix` that extends or implements `port`, a class or an interface imported from `module`, has its `method` as a boundary. |
| `package-exports-file` | string | absent: no file | A tracked file listing the value exports of the known packages. |

The default `framework-methods`:

| Segment | Methods |
| --- | --- |
| `/policy/` | `getCustomActionRule`, `onBeforeCreate`, `onBeforeDelete`, `onBeforeGet`, `onBeforeGetList`, `onBeforePartialUpdate`, `onBeforeUpdate` |
| `/subscriber/` | `onAfter<E>` and `onAfterError<E>` for each event `E` of `Commit`, `Create`, `Custom`, `Delete`, `Get`, `GetList`, `GetMany`, `PartialUpdate`, `Rollback`, `Update`; `onBeforeError<E>` for each of them; `onBefore<E>` for each of them but `Commit` and `Rollback` |
| `/guard/` | `canActivate`, `getAuthenticateOptions`, `getRequest`, `handleRequest`, `logIn` |
| `/filter/` | `catch` |
| `/interceptor/` | `intercept` |
| `/migration/typeorm/` | `down`, `up` |
| `/resolver/` | `resolve` |
| `/pipe/` | `transform` |
| `/middleware/` | `use` |
| `/strategy/` | `validate` |

`package-exports-file` names a Rust source table, generated from the installed packages, of this
shape (`//` comments are ignored):

```rust
pub const PACKAGES: [(&str, &[&str]); 2] = [
    ("@nestjs/common", &NEST_COMMON),
    ("@scope/automator", &AUTOMATOR),
];

const NEST_COMMON: [&str; 2] = ["BadRequestException", "NotFoundException"];
const AUTOMATOR: [&str; 1] = ["ErrorString"];
```

An import of a listed package resolves only through its list: a name it does not list is no
export. Without the option, the known packages are `exception-packages` and the message factory's
package, and each exports every name imported from it. A named file that is not tracked, or not of
that shape, is an error that says what is wrong.

```toml
[rules]
"nestjs/wrappers" = { level = "error", options = { package-exports-file = "tools/packages.rs" } }
```

An exception names a raw construction by its target:

```toml
[[exception]]
rule = "nestjs/wrappers#raw-nest-exception"
target = "src/modules/health/service.ts: HealthService.read: new ServiceUnavailableException({ status: EHealthStatus.DOWN })"
reason = "the readiness probe answers with the status object"
```

## Example finding

```text
error nestjs/wrappers#one-call-forwarding src/modules/book/service.ts:4
  src/modules/book/service.ts:4 load only forwards to this.store.load; inline the call
```

## How to fix it

- `one-call-forwarding`, `intrinsic-alias`: call the target directly where the wrapper is called
  and remove the wrapper.
- `throw-only-error`: construct the complete error where the failure is decided.
- `raw-nest-exception`: pass a message of the message factory as the exception's first argument,
  or add an exception naming the construction's target when the response must be another shape.

# `nestjs/api-readiness`

The API role becomes ready only after its readiness owner and its listener, in that order.

## What it checks and why

A role that reports ready before its dependencies are checked, or before it listens, receives traffic
it cannot serve. The rule pins the startup sequence: assert readiness, start the listener, then mark
the role ready, each exactly once and on every path through the startup function.

- `readiness-owner`: every file of `readiness-files` is tracked; each missing one is reported at its
  path.
- `startup-order`: `startup-file` is tracked (a missing file is the only `startup-order` finding), and
  declares a top-level function `entrypoint` (exported, default-exported or not; the first one counts)
  with a body. Its body, or the body of one local startup operation, must hold, in one block that
  always runs:
  1. `await <application>.<get>(<readiness-provider>).<assert-ready>(...)`,
  2. `await <application>.<listen>(...)`,
  3. `<lifecycle>.<mark-ready>()` with no argument and not awaited,

  in that order, and no other of these calls anywhere outside nested functions, classes and object
  methods. A block always runs when the statements before it in its enclosing block do not
  unconditionally `return`, `throw` or `break` out of it; `try` blocks count, `catch` and `finally`
  blocks are followed for their exits. A local startup operation is a `const` binding of an `async`
  arrow function with a block body and no parameters, declared in the entrypoint's body before its
  only reference, which is `await operation();` (also `await operation?.()`) as an always-run
  statement. An arrow with one of the calls that is bound in any other way fails the rule. The finding
  is located at the entrypoint's declaration.

A startup file without a syntax tree is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `readiness-owner`,
`startup-order`. Findings carry no target.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `readiness-files` | array of string | `["src/runtime/api/readiness/index.ts", "src/runtime/api/readiness/module.ts", "src/runtime/api/readiness/service.ts"]` | The files of the readiness owner. |
| `startup-file` | string | `"src/runtime/api/function/start.function.ts"` | The file of the startup function. |
| `entrypoint` | string | `"runtimeApiStart"` | The startup function. |
| `application` | string | `"roleApplication"` | The application variable. |
| `readiness-provider` | string | `"RuntimeApiReadinessService"` | The readiness provider class. |
| `lifecycle` | string | `"roleLifecycle"` | The lifecycle variable. |
| `role-name` | string | `"Runtime API"` | The name of the role the messages print. |
| `calls` | table | `{ get = "get", assert-ready = "assertReady", listen = "listen", mark-ready = "markReady" }` | The method names of the calls. |

```toml
[rules]
"nestjs/api-readiness" = { level = "error", options = { entrypoint = "start", calls = { listen = "listen" } } }
```

## Example finding

```text
error nestjs/api-readiness#startup-order src/start.ts:3
  src/start.ts start must await app.get(Readiness).assertReady, then await app.listen, then call lifecycle.markReady in that order in the entrypoint or one locally bound, once-awaited startup operation
```

## How to fix it

Add the missing readiness owner files. In the startup function, await the readiness assertion, then
await the listener, then mark the role ready, once each, outside any condition, loop or callback; or
move the three into one `const` async arrow without parameters that the function awaits once.

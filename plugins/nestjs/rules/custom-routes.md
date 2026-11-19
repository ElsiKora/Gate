# `nestjs/custom-routes`

Custom HTTP handlers use the CRUD automator's custom route decorator, never a raw route decorator of
the `NestJS` common package.

## What it checks and why

The automator registers a custom handler, its `OpenAPI` metadata and its guards through one decorator.
A handler declared with a raw route decorator such as `@Get` bypasses that registration, so its route
is missing from the generated contract and its guards.

In every `.ts` file below `layout.source-root` (declaration files and `[languages.typescript] skip-folders` left out),
nothing may name a route decorator of `package` or of a subpath of it (`package/...`):

- a named import or a named `export { ... } from` of a route decorator, reported at its specifier;
- an `export * from` the package, reported as `re-exported`;
- a read of a route decorator from a namespace, default or `import x = require(...)` binding of the
  package: `x.Get`, `x["Get"]` or `const { Get } = x`.

A file without a syntax tree is skipped; `typescript/parse` reports it.

Limits: the rule reads syntax, not values, and these escape it:

- a `require("…")` call (only `import x = require("…")` binds the package);
- a copy of a binding (`const y = x; y.Get`) and a destructuring outside a variable declaration
  (an assignment pattern or a parameter);
- a computed key that is not a string literal (`x[name]`, a template literal);
- a dynamic `import("…")`;
- scope: bindings are matched by name, so an inner variable that shadows the namespace name is still
  read as the namespace.

Scope: project. Language: `typescript`. Reads history: no. One aspect (`""`). Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `package` | string | absent: the layout's package (`layout.nest-common-package`) | The package whose route decorators are raw. |
| `route-decorators` | array of string | `["All", "Copy", "Delete", "Get", "Head", "Lock", "Mkcol", "Move", "Options", "Patch", "Post", "Propfind", "Proppatch", "Put", "RequestMapping", "Search", "Sse", "Unlock"]` | The route decorators of the package. |
| `replacement` | string | `"@ApiRouteCustom"` | The decorator the message recommends. |

```toml
[rules]
"nestjs/custom-routes" = { level = "error", options = { replacement = "@ApiRouteCustom" } }
```

## Example finding

```text
error nestjs/custom-routes src/modules/book/book.controller.ts:2
  src/modules/book/book.controller.ts uses the raw Nest @Get route decorator; declare custom handlers with @ApiRouteCustom
```

## How to fix it

Declare the handler with the automator's custom route decorator and remove the raw route decorator's
import.

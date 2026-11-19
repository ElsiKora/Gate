# `nestjs/native-functions`

Every asynchronous method of a native service is one CRUD automator function.

## What it checks and why

The automator wraps each function of a native service in its transaction and error handling. A
public asynchronous method without a function decorator runs outside that handling; two decorators,
a decorator without a transaction mode, or a decorator on a synchronous method change what callers
receive.

Native services are found as in `nestjs/native-repository` (`framework-package`, `service-decorator`,
`service-base`, across files), over the `.ts` files below `layout.source-root`. A class in a service
file (`service-file-names`) whose constructor injects a repository through `inject-repository` is
treated as a native service too.

- `#api-service`: a native service without its own `service-decorator` call whose options name
  `entity-option` in an object literal (directly or through a top-level variable); and a service-file
  class that injects a repository but is no native service. Reported at the class, its decorators
  included.
- `#undecorated-method`: a non-private asynchronous method of a native service without any of
  `function-decorators`. A method is asynchronous when it is `async`, declares a `Promise` return type
  or declares none. Constructors, accessors, overload signatures, abstract methods,
  `lifecycle-methods` and `inherited-methods` are exempt. A non-private static method or
  function-valued property is reported too, since neither can be an automator function.
- `#function-decorator-count`: every function decorator after the first on one member.
- `#transaction-mode`: a function decorator whose first argument does not name the key path
  `transaction-option` (`transaction: { mode }`) in object literals.
- `#synchronous-function`: a function decorator on anything but an asynchronous method.
- `#function-owner`: a function decorator on a member of a class that is no native service.
- `#private-custom`: `custom-function-decorator` on a private member (`private` or a `#` name).

Scope: project. Language: `typescript`. Reads history: no. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `framework-package` | string | absent: the layout's package (`layout.automator-package`) | The package of the service and function decorators. |
| `service-decorator` | string | `"ApiService"` | The class decorator of a native service. |
| `service-base` | string | `"ApiServiceBase"` | The base class of a native service. |
| `function-decorators` | array of string | `["ApiFunctionCustom", "ApiFunctionStep"]` | The function decorators; at least one. |
| `custom-function-decorator` | string | `"ApiFunctionCustom"` | The function decorator a private member never carries. |
| `lifecycle-methods` | array of string | the five `NestJS` lifecycle hooks | Methods exempt from being functions. |
| `inherited-methods` | array of string | `["create", "delete", "get", "getList", "getMany", "update"]` | Inherited functions, exempt. |
| `entity-option` | string | `"entity"` | The service decorator option that names the entity. |
| `transaction-option` | array of string | `["transaction", "mode"]` | The key path a function decorator's options name. |
| `service-file-names` | array of string | `["service.ts", ".service.ts"]` | Service file names; an entry starting with `.` is a suffix. |
| `inject-repository` | `{ package, decorator }` | `{ package = "@nestjs/typeorm", decorator = "InjectRepository" }` | The repository injection decorator. |

```toml
[rules]
"nestjs/native-functions" = { level = "error", options = { inherited-methods = ["create", "get"] } }
```

## Example finding

```text
error nestjs/native-functions#undecorated-method src/modules/book/book.service.ts:4
  src/modules/book/book.service.ts:4 BookService.lend is a non-private asynchronous method of a native service without ApiFunctionCustom or ApiFunctionStep
```

## How to fix it

Decorate each public asynchronous method with one function decorator that names its transaction mode,
make helpers private (and decorate private participants with the step decorator), and give each native
service its own service decorator naming its entity.

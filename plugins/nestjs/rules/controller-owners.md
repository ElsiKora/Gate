# `nestjs/controller-owners`

Every controller is a registered semantic owner with exact metadata.

## What it checks and why

A controller is the public face of one owner: one entity, one primary service, one set of routes. When
a controller is not registered, binds another entity than its service, keeps helper logic, or exposes
contracts named after someone else, the API stops saying who owns what. The registry, the
`controllers` option, writes each controller's owner down once; the rule holds the code to it.

The sources are the `.ts` files (not `.d.ts`) below `layout.source-root`, outside the
`[languages.typescript] skip-folders`. A controller is a top-level class named `…Controller`, or
decorated with `ApiController` of `layout.automator-package` or `Controller` of
`layout.nest-common-package`. Names resolve through imports, re-exports and aliases to the class that
declares them.

- `#registry-contract`: a row breaks the registry's own contract: its `path` does not end with
  `/controller.ts` or repeats another row's; its `controller-name` does not end with `Controller` or
  repeats; its `primary-service-name` does not end with `Service` or it has no `primary-service-path`; its
  `classification` is not `CUSTOM_USE_CASE`, `ENTITY_RESOURCE`, `EXTERNAL_NOUN` or `PLAIN`; a
  `semantic-owner-name` that is no parent family of the controller's name, or comes without a
  `decision-id`; a distinct primary service entity outside `CUSTOM_USE_CASE` or without both of its
  fields; `entity-name` and `entity-path` not both strings (both `null` for `PLAIN`); a `CUSTOM_USE_CASE`
  or `EXTERNAL_NOUN` row without a `decision-id` of the form `D-…` (upper-case letters, digits and
  hyphens); an `EXTERNAL_NOUN` row outside `layout.modules-root`/`external-noun-plane`. Reported at
  the options file.
- `#registration`: a controller of a file no row names (at the class); a registered file that does not
  hold exactly the one registered controller (at the file); a row whose file holds no controller (at
  the options file). A registered file that does not parse is left to `typescript/parse`.
- `#api-metadata`: a `PLAIN` controller that is not one Nest `Controller` without `ApiController`, or
  another controller that is not one `ApiController` without Nest `Controller`. For an `ApiController`:
  its one entity type argument, its `implements IApiControllerBase<…>` (the primary service's entity),
  a configuration variable typed `IApiControllerProperties<…>`, and the configuration object's `entity`
  and `name` must match the row. The expected `name` is `semantic-owner-name`, or `controller-name`
  without its last ten characters.
- `#service-owner`: the controller does not declare exactly one constructor, does not inject the
  primary service exactly once from `primary-service-path`, or names that parameter otherwise than
  `service-member`; the primary service is not a source class that binds its entity through
  `extends ApiServiceBase<…>` or `@ApiService({ entity })`, binds another one (reported at the
  service), or resolves the entity from another owner than the controller.
- `#route-handler-only`: a member other than a constructor is not a method with a route decorator: one
  of `nest-route-decorators` of `layout.nest-common-package`, or an automator decorator whose name
  `automator-route-pattern` matches. Reported at the member.
- `#owned-contract-prefix`: a public member of the controller or its primary service exposes, in its
  signature, a type declared in the owner's own folder whose name ends with `DTO`, or starts with `I` or
  `T` and a capital, without the owner's prefix (`semantic-owner-name`, or the class name without
  `Controller` or `Service`; `I` or `T` before it for interfaces and types). Reported at the member.
- `#custom-service-entity`: a controller whose primary service binds another entity than the
  controller must disable every route of `layout.generated-routes` with
  `generation: { isEnabled: false }`, load no `relations` in its configuration or in any
  `ApiRouteCustom`, extend nothing and take one constructor dependency.

Messages start with the file and line of the class. Two identical findings are reported once.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `api-metadata`,
`custom-service-entity`, `owned-contract-prefix`, `registration`, `registry-contract`,
`route-handler-only`, `service-owner`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `controllers` | list of rows | `[]` | The registry; with none, every controller is unregistered. |
| `external-noun-plane` | string | `"public"` | The module plane below `layout.modules-root` that holds `EXTERNAL_NOUN` controllers. |
| `service-member` | string | `"service"` | The name of the constructor parameter that injects the primary service. |
| `nest-route-decorators` | list of strings | `["All", "Delete", "Get", "Head", "Options", "Patch", "Post", "Put", "Search"]` | The Nest route decorators. |
| `automator-route-pattern` | regular expression | `"^ApiRoute[A-Za-z0-9]*$"` | The names of the automator's route decorators. |

A row of `controllers`:

| Key | Meaning |
| --- | --- |
| `path` | The controller file, ending in `/controller.ts`. |
| `controller-name` | The controller class, ending in `Controller`. |
| `classification` | `CUSTOM_USE_CASE`, `ENTITY_RESOURCE`, `EXTERNAL_NOUN` or `PLAIN`. |
| `primary-service-name` | The primary service class, ending in `Service`. |
| `primary-service-path` | The file of the primary service class. |
| `entity-name` | The entity the controller serves; `null` for `PLAIN`. |
| `entity-path` | The file of that entity; `null` for `PLAIN`. |
| `primary-service-entity-name` | The entity of a primary service bound to another entity (`CUSTOM_USE_CASE`). |
| `primary-service-entity-path` | The file of that entity. |
| `semantic-owner-name` | The parent controller family the controller belongs to. |
| `decision-id` | The accepted decision (`D-…`) behind the classification or the owner. |

A key the table does not list, or a value that is not a string, is a configuration error. Every key
may be left out: the rule reports a row that lacks one it needs, as `#registry-contract`. A `null`
counts as left out, except for `entity-name` and `entity-path`, where it is the value of a `PLAIN`
row and leaving them out is a finding. The messages name the keys in their camelCase spelling
(`controllerName`, `decisionId`, …).

```toml
[rules]
"nestjs/controller-owners" = { level = "error", options-file = "gate/options/controller-owners.json" }
```

```json
{
  "controllers": [
    {
      "path": "src/modules/internal/book/controller.ts",
      "controller-name": "InternalBookController",
      "classification": "ENTITY_RESOURCE",
      "primary-service-name": "BookService",
      "primary-service-path": "src/modules/domain/book/service.ts",
      "entity-name": "BookEntity",
      "entity-path": "src/modules/domain/book/entity.ts"
    }
  ]
}
```

## Example finding

```text
error nestjs/controller-owners#route-handler-only src/modules/internal/book/controller.ts:11
  src/modules/internal/book/controller.ts:11 InternalBookController.helper is not a route-decorated controller handler; move helper logic to its semantic owner
```

## How to fix it

Register each controller once, bind it and its primary service to the registered entity, keep only
route handlers in it, and name the contracts it owns after its owner. Remove a row when its controller
is removed.

# `nestjs/owner-boundaries`

Source files keep the boundaries of their architecture owner.

## What it checks and why

A service divides its source into owners: the domain, the API planes, infrastructure, shared code.
Each owner exposes its own source through its barrels, only the API owners serve HTTP, routes are
composed from the plane constants so that a plane can move without editing every controller, the
domain's modules are exported only by the domain itself, child resources stay nested below the
resource that owns them, and a domain service keeps the ORM's entity manager out of its public
methods. Each of these breaks silently when one file crosses its owner's boundary.

Every `.ts` file below `layout.source-root` is read, declaration files included, in every folder: the walk skips none of
`[languages.typescript] skip-folders`. A file belongs to the first `owner-roots` entry that equals its path or contains it. A
specifier names an owner through `layout.path-aliases` or as a path relative to the file; a package
or an absolute path names none. A file without a syntax tree is skipped; `typescript/parse` reports
it.

Scope: project. Language: `typescript`. Reads history: no. Findings are located at the file and carry
no target.

### `owner-barrel`

An `index.ts` that re-exports (`export *`, `export * as`, a non-empty `export { ... } from`, type-only
too) a module of another owner. Each re-exported module is reported once.

### `controller-owner`

A file outside the `api-owners` that declares a class with one of `controller-decorators`.

### `api-plane-route`

A controller's routes are read from its decorator: for a decorator of `module-decorator`'s package
(Nest's `Controller`) the first argument unless it is an object literal, otherwise the `path`
property of the options object. Routes are followed through `const` names, arrays and their spreads,
object spreads and literal computed keys. A file is reported when:

- the literal start of a route (a string, a template's head, the left side of `+`) starts with one of
  `plane-routes` as a whole segment;
- the file lies below `internal-owner` and one of its controllers has a route that does not start
  with `internal-base` (the `export` path imported from one of its `modules`), or has no route;
- the literal start of a route starts with one of `retired-routes`.

### `nested-resource`

- A file named `<name><flat-key-controller-suffix>`, where `<name>` holds only `a-z`, `0-9` and `-`.
- A file whose path holds `/<flattened-resource>` followed by `/` or its end.
- A controller below `internal-owner` whose route, as far as it is known, does.

### `domain-module-forwarding`

A `module.ts` outside `domain-owner` whose `module-decorator` exports, directly or through
`forwardRef(() => ...)`, an import named `...Module` from a module of the domain owner.

### `entity-manager-exposure`

In a `domain-service-file` below `domain-owner`, a public method of a class or an object literal with
a parameter whose type holds `entity-manager`, directly or through local type aliases and
interfaces. Constructors, accessors, `private`, `protected` and `#private` methods are left out.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `owner-roots` | array of string | required | The owner roots, in the order a file is matched. |
| `api-owners` | array of string | required | The owner roots that may declare controllers. |
| `domain-owner` | string | required | The owner of the domain modules and services. |
| `internal-owner` | string | required | The folder whose routes start with `internal-base`. |
| `plane-routes` | array of string | required | Route segments that must not start a route literally. |
| `retired-routes` | array of string | `[]` | Retired route segments that must not start a route literally. |
| `internal-base` | `{ modules, export }` | required | The modules that export the internal base, and its export path. |
| `flattened-resources` | array of string | `[]` | Child resources (`parent/child`) that must stay nested. |
| `flat-key-controller-suffix` | string | absent: not checked | The file name suffix of a flat key controller. |
| `controller-decorators` | array of `{ package, name }` | absent: `Controller` of `layout.nest-common-package` and `ApiController` of `layout.automator-package` | The decorators that declare a controller. |
| `module-decorator` | `{ package, name }` | absent: `Module` of `layout.nest-common-package` | The decorator that declares a module; its package also exports `forwardRef`. |
| `entity-manager` | `{ package, name }` | absent: `EntityManager` of `layout.typeorm-package` | The native entity manager type. |
| `domain-service-file` | string | `"service.ts"` | The file name of a domain service. |

The rule also reads `layout.source-root` and `layout.path-aliases` from the `[plugins.nestjs]
options` block.

```toml
[rules."nestjs/owner-boundaries"]
level = "error"
options = { owner-roots = ["src/modules/domain", "src/modules/public", "src/modules/internal", "src/shared"], api-owners = ["src/modules/public", "src/modules/internal"], domain-owner = "src/modules/domain", internal-owner = "src/modules/internal", plane-routes = ["public", "internal"], internal-base = { modules = ["@shared/constant"], export = ["ROUTE_CONSTANT", "INTERNAL"] } }
```

## Example findings

```text
error nestjs/owner-boundaries#owner-barrel src/modules/internal/book/index.ts
  src/modules/internal/book/index.ts re-exports src/modules/domain source across its owner barrel
error nestjs/owner-boundaries#api-plane-route src/modules/internal/book/controller.ts
  src/modules/internal/book/controller.ts must compose its route from ROUTE_CONSTANT.INTERNAL
error nestjs/owner-boundaries#entity-manager-exposure src/modules/domain/book/service.ts
  src/modules/domain/book/service.ts exposes native EntityManager in public method save(manager)
```

## How to fix it

Re-export only the barrel's own owner, and let consumers import another owner through its alias.
Move controllers into an API owner and compose their routes from the plane constants and the internal
base. Import a domain module where it is used instead of exporting it from another owner. Nest a
child resource's files and routes below its owning resource. Keep the entity manager inside the
domain service, behind its own methods.

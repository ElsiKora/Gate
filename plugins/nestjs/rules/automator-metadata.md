# `nestjs/automator-metadata`

Entity, DTO and route metadata follow the CRUD automator's contracts.

## What it checks and why

The CRUD automator generates DTOs, validation and documentation from metadata. A persisted field
without an explicit description, a DTO property with no automator decorator or with two, or a handler
the automator does not own produces a contract nobody wrote down: a field leaks into a response, a
request goes unvalidated, a route goes undocumented.

The rule builds three inventories, each entry keyed `Owner.member` with no path, holding the contract
it has and the one it must have:

- **entityProperties**: every property with one of the persisted decorators
  (`layout.typeorm-column-decorators`, `layout.typeorm-relation-decorators` and
  `layout.typeorm-other-persisted-decorators`, of `layout.typeorm-package`) of an `@Entity` or
  `@ChildEntity` class of a file below `layout.modules-root`/`layout.domain-folder` ending with
  `entity-file-suffix`, and of the classes such a class extends, followed through `base-depth` bases
  beyond the direct one. It must carry one `ApiPropertyDescribe` of `layout.automator-package` with a
  `type` (`DESCRIBED_EXPLICIT`).
- **dtoProperties**: every property of a class whose name ends with `dto-suffix`, in a file whose path
  holds `dto-folder`. It must carry one automator `ApiProperty…` decorator (or its `custom-decorators`
  entry, called by that name) and no `layout.validator-package` validator the automator already adds.
- **routes**: the routes `layout.generated-routes` a controller (a file ending with
  `controller-file-suffix`) generates through `ApiController`, minus those its configuration turns off
  (and the update routes when `routes-helper` is called with `update-flag` set to `false`); these are
  listed for their keys only. And every handler of the controller, which must carry exactly one owner,
  `ApiRouteCustom` (`ApiMethod` counts as an owner but is not the expected one), and no other local
  decorator than `always-allowed-route-decorators`, its `documentation-decorators` and, on
  `health-check-route`, `health-check-decorator`, which that handler must carry. With a
  `parameter-decorators` entry, its parameters' decorators must be exactly the entry's.

The three inventories read every `.ts` file below `layout.modules-root` in every folder: that listing
skips none of `[languages.typescript] skip-folders`. The bases an entity extends are followed through
the TypeScript source listing, which skips those folders: an entity file in a skipped folder has its
own properties listed but its bases not followed, and a base class in a skipped folder is not followed.

Decorators are matched by their import, through aliases and namespaces; a name in `decorator-modules`
counts only when imported from its module. Default imports are not read.

- `#entity-description`, `#dto-decorator`, `#route-contract`: an entry's contract differs from the
  expected one. The message prints both as JSON with sorted keys. Reported at the member.
- `#route-owner`: a handler does not have exactly one owner. Reported at the handler.
- `#inventory-identity`: two entries of an inventory share a key. Reported at the first.
- `#entity-property-decorator`: an entity property carries an automator property decorator other than
  `ApiPropertyDescribe`. Reported at the decorator.
- `#controller-decorator`: a controller class carries a decorator outside
  `controller-class-decorators`. Reported at the decorator.
- `#policy-key`: an entry of `hidden-properties`, `buffer-properties`, `custom-decorators`,
  `documentation-decorators` or `parameter-decorators` names no inventory entry. Reported at the line of
  the entry's key in the options file, or at `gate.toml` when the options are written there. The
  messages name the options by the automator's own inventory names (`hiddenProperties`,
  `bufferProperties`, `customDecorators`, `documentationExceptions`, `parameterDecorators`).

Two identical findings are reported once.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `controller-decorator`,
`dto-decorator`, `entity-description`, `entity-property-decorator`, `inventory-identity`, `policy-key`,
`route-contract`, `route-owner`. Findings carry no target.

## Options

The five entry kinds state an expected contract, so they are options, not exceptions: an exception
would hide every other difference of the same entry. Each entry keeps its `reason` and, optionally, the
`decision` that settled it.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `hidden-properties` | map `Entity.property` → `{ type, reason, decision? }` | `{}` | Entity properties hidden from generated DTOs: they must also set `isAutoDtoEnabled: false` and this exact `type`. |
| `buffer-properties` | map `Entity.property` → `{ reason, decision? }` | `{}` | Buffer properties hidden from generated DTOs: they must also set `isAutoDtoEnabled: false`. |
| `custom-decorators` | map `DTO.property` → `{ decorator, reason, decision? }` | `{}` | An owner-local decorator that counts as the property's automator decorator. |
| `documentation-decorators` | map `Controller.method` → list of `{ decorator, reason, decision? }` | `{}` | Documentation decorators a custom handler may carry. |
| `parameter-decorators` | map `Controller.method` → `{ parameters, reason, decision? }` | `{}` | The exact decorator names of each parameter of a handler (`this` has none; a rest parameter counts). |
| `health-check-route` | `Controller.method` | none | The one handler that carries `health-check-decorator`; with none, no handler may. |
| `health-check-decorator` | string | `"HealthCheck"` | The health-check decorator. |
| `routes-helper` | `{ name, update-flag }` | none | A helper call that generates a controller's routes; with none, no call does and the update routes are always expected. |
| `entity-file-suffix` | string | `"/entity/entity.ts"` | The ending of a domain entity file's path. |
| `dto-folder` | string | `"/dto/"` | The path fragment of a DTO file. |
| `dto-suffix` | string | `"DTO"` | The ending of a DTO class's name. |
| `controller-file-suffix` | string | `"/controller.ts"` | The ending of a controller file's path. |
| `controller-class-decorators` | list of strings | `["ApiController", "ApiControllerObservable", "ApiControllerSecurable", "ApiTags", "Controller", "UsePipes"]` | The class decorators a controller may carry. |
| `always-allowed-route-decorators` | list of strings | `["RouteConfig"]` | The handler decorators every handler may carry. |
| `decorator-modules` | map name → module | `ApiBody`, `ApiTags` of `@nestjs/swagger`; `Controller`, `UsePipes` of `@nestjs/common`; `HealthCheck` of `@nestjs/terminus`; `RouteConfig` of `@nestjs/platform-fastify` | Framework decorators that count only when imported from their module. |
| `base-depth` | integer | `8` | How many bases of an entity class are followed beyond its direct base. |

```toml
[rules]
"nestjs/automator-metadata" = { level = "error", options-file = "gate/options/automator-metadata.json" }
```

```json
{
  "hidden-properties": {
    "BookEntity.owner": { "type": "EApiPropertyDescribeType.RELATION", "reason": "loaded on demand" }
  },
  "health-check-route": "HealthController.getHealth"
}
```

## Example finding

```text
error nestjs/automator-metadata#dto-decorator src/modules/internal/book/dto/request.dto.ts:5
  automator metadata dtoProperties BookDTO.title expected {"decoratorCount":1,"duplicateValidatorCount":0}; received {"decoratorCount":0,"duplicateValidatorCount":0}
```

## How to fix it

Describe every persisted field with one explicit `ApiPropertyDescribe`, give every DTO property one
automator property decorator, and let `ApiRouteCustom` own every handler. Record a deliberate difference
as an entry of the matching option with its reason, and remove an entry once its member is gone.

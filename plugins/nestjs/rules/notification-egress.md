# `nestjs/notification-egress`

Egress has a registered owner and notifications leave through consumer classes.

## What it checks and why

Every request that leaves the application is a path for data to leave with it. The options register
who may send: each egress owner is a source root with its classes and the decision that admits it, and
one shared transport is the outbound edge other code sends through. Notifications are a class of their
own: they leave only from a class extending the consumer port, so a business fact, not a random call
site, decides that a message goes out.

The sources are the `.ts` files below `layout.source-root` (not `.d.ts`), outside the folders of
`[languages.typescript] skip-folders`. A file belongs to the owner with the deepest root that contains
it; roots may nest. An egress use of a file is:

- a runtime load of one of `egress.modules` or a module below it (`node:https`,
  `@aws-sdk/client-sesv2/dist`): an import, re-export, `require` or dynamic `import()`, where a
  type-only import is no load;
- a runtime import, named or default, of one of `egress.transport.symbols` whose source owner, chased
  through barrels, is a file below the transport root;
- a value reference, called or not, to one of `global-egress-functions` that no local binding shadows,
  or to that member of `globalThis`, static or by a string key; `typeof fetch` in a type is no use.

The aspects:

- `#unclassified-egress`: a file with an egress use lies below no owner root. Below the transport root,
  such a file passes when it only loads transport modules and imports transport symbols. Reported at its
  first offending use.
- `#direct-notification`: a file outside every owner of `notification-class` imports, type-only imports
  included, a symbol whose source owner is a file of such an owner, or a class other than the consumer
  port that such an owner's classes extend or implement, or imports a module that resolves to a file of
  such an owner. Files named in `composition-files`, files declaring a class that extends or implements
  `consumer`, and an owner's `senders` pass. Only top-level import declarations count. Without
  `notification-class` this aspect is off.
- `#stale-owner`: an owner without a file that uses an egress point; a sender that loads nothing of its
  owner; a transport root without a file, a transport module no file below the root and outside every
  owner loads, a transport symbol no file below the root declares; a platform client no file loads.
  Reported in the options file: an owner and its senders at the owner's line, the transport and
  platform entries at their first occurrence in the `egress` entry. With inline options they are
  reported at `gate.toml` without a line.
- `#unclassified-package`: a runtime load, in any file, of a package whose name starts with one of
  `egress.families` and is neither an egress module nor a platform client.

The run stops on options that contradict themselves: a class outside `owner-classes`, a notification
class without `consumer` and `consumer-label`, a root that is not a unique folder below the source root
ending with `/`, a transport module that is not an egress module, a platform client or excluded module
that is also an egress module, an excluded module without a reason, an empty or repeated family, or a
decision that the `decisions` index does not list as `accepted`.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `direct-notification`,
`stale-owner`, `unclassified-egress`, `unclassified-package`. Findings carry no target. The rule is in
no preset; a project enables it with its own options.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `owner-classes` | list of strings | required | The closed set of owner classes, each once. |
| `notification-class` | string | none | The class of the owners that send notifications; without it `#direct-notification` is off. |
| `consumer` | `{ name, path }` | none | The consumer port; required with `notification-class`. |
| `consumer-label` | string | none | What a class extending the consumer port is called in messages; required with `notification-class`. |
| `egress.modules` | list of strings | required | The egress modules. |
| `egress.families` | list of strings | `[]` | Package-name prefixes closed over their members. |
| `egress.platform` | list of `{ module, decisions }` | `[]` | Packages of a family that serve the platform, with their decisions. |
| `egress.excluded` | list of `{ module, reason }` | `[]` | Network-capable modules kept off the egress list; only validated. |
| `egress.transport` | `{ root, modules, symbols, decision }` | none | The shared outbound transport. |
| `owners` | list of `{ root, classes, decision, senders }` | required | The egress owners; `senders` is a list of `{ path, decision }`. |
| `composition-files` | list of strings | `["index.ts", "module.ts"]` | File names that compose owners without sending. |
| `global-egress-functions` | list of strings | `["fetch"]` | Global functions that send. |
| `decisions` | path | none | A Markdown decision index; without it every decision counts as accepted. Rows start with ``| `D-`` and have at least four cells, the status in the second. |

```toml
[rules]
"nestjs/notification-egress" = { level = "error", options-file = "gate/options/nestjs/notification-egress.json" }
```

```json
{
  "owner-classes": ["NOTIFICATION", "PAYMENT"],
  "notification-class": "NOTIFICATION",
  "consumer": { "name": "DeliveryConsumer", "path": "src/common/delivery/consumer.ts" },
  "consumer-label": "delivery consumer",
  "egress": { "modules": ["node:https"] },
  "owners": [{ "root": "src/infrastructure/mail/", "classes": ["NOTIFICATION"], "decision": "D-0001" }]
}
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/notification-egress#unclassified-egress src/modules/user/user.service.ts:2
  src/modules/user/user.service.ts sends outside the application through the global fetch, but lies under no egress owner of the nestjs/notification-egress policy; move it under its owner or register its root with a class and an accepted decision
```

## How to fix it

Send through the owner that the options register for the purpose, or register a new owner with its
class and an accepted decision. Send notifications from a class extending the consumer port. Remove
entries the source no longer needs.

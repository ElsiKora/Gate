# `nestjs/kind-layout`

Source and unit test files sit where their kind belongs: kind folders hold one kind, controllers
and owner files sit at their resource root, subscribers sit in a subscriber folder and are named by
responsibility, helper folders exist only in the shared owner, and unit tests of module areas sit in
the modules namespace.

## What it checks and why

A kind folder (`dto/`, `type/`, …) tells a reader what every file in it is. A kind folder inside a
role folder or inside a folder of another kind, an owner file inside a kind folder, or a subscriber
outside its folder breaks that promise, and the next file lands in the wrong place too.

The rule walks the folders below `layout.source-root` that hold a tracked file, and the tracked
`.ts` files (declaration files included), leaving out `[languages.typescript] skip-folders`. The
module areas of `unit-placement` (the folders directly inside `layout.modules-root`) and the check
that an area has no folder of its own below `layout.source-root` see every folder.

- **`nested-kind-folder`.** A folder with a `layout.role-folders` segment below the source root and
  a `layout.kind-folders` segment after it (`book/guard/dto`), reported at each such folder.
- **`mixed-kind-folder`.** A folder whose last segment is a `layout.exclusive-kind-folders` entry
  and that lies below a folder of another such kind (`common/utility/type`). Only the last segment
  is judged, so the folders below it are not reported again.
- **`utility-placement`.** A folder named `utility-folder` that is not below `utility-root`.
- **`controller-placement`.** Below `layout.modules-root`: a file inside a `controller-folder`
  folder, and a file named `<kebab>.<controller-folder>.ts`.
- **`owner-file-placement`.** An `owner-files` file with an exclusive kind folder or a role folder
  among its ancestors; the message names the nearest one.
- **`subscriber-placement`.** A file named `<subscriber-folder>.ts`, even inside a subscriber
  folder, and a `*.<subscriber-folder>.ts` file with no `subscriber-folder` ancestor.
- **`subscriber-name`.** A subscriber directly inside a `subscriber-folder` folder named by one of
  `lifecycle-subscriber-names`, or by one of `lifecycle-subscriber-prefixes` followed by a kebab word
  (`on-before-create`).
- **`unit-namespace`.** A file below `unit-root` whose first folder is a folder directly below
  `layout.modules-root`, unless that folder is one of `unit-categories`, the file is below one of
  `unit-exempt-prefixes`, or the source root has a folder of that name. The message names the path
  in the modules namespace.

Every finding is located at the folder or file it names.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `controller-placement`,
`mixed-kind-folder`, `nested-kind-folder`, `owner-file-placement`, `subscriber-name`,
`subscriber-placement`, `unit-namespace`, `utility-placement`. Findings carry no target.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `utility-folder` | string | `"utility"` | The name of a helper folder: a single name. |
| `utility-root` | string | `"src/shared"` | The only folder below which helper folders may sit; `""` lets them sit anywhere. |
| `controller-folder` | string | `"controller"` | The controller kind: its folder name and file kind, a single name. |
| `owner-files` | array of string | `["controller.ts", "module.ts", "service.ts"]` | The files that sit at an owner root. |
| `subscriber-folder` | string | `"subscriber"` | The subscriber kind: its folder name and file kind, a single name. |
| `lifecycle-subscriber-names` | array of string | `["create", "delete", "function", "route", "update"]` | Subscriber names that describe a layer or lifecycle operation. |
| `lifecycle-subscriber-prefixes` | array of string | `["on-after-", "on-before-"]` | Prefixes that, followed by a kebab word, name a subscriber by lifecycle. |
| `unit-root` | string | `"test/unit"` | The root of the unit tests. |
| `unit-categories` | array of string | `["api", "fixture", "scripts"]` | Folders below the unit root that are categories, not module areas. |
| `unit-exempt-prefixes` | array of string | `[]` | Path prefixes below the unit root that are not judged. |

The rule reads `layout.source-root`, `layout.modules-root`, `layout.kind-folders`,
`layout.exclusive-kind-folders`, `layout.role-folders` from
`[plugins.nestjs] options`. The two kind lists are distinct: `nested-kind-folder` reads
`kind-folders`, the other aspects read `exclusive-kind-folders`.

```toml
[rules]
"nestjs/kind-layout" = { level = "error", options = { utility-root = "src/shared" } }
```

## Example finding

```text
error nestjs/kind-layout#owner-file-placement src/modules/domain/book/dto/module.ts
  src/modules/domain/book/dto/module.ts is inside a dto folder; module.ts belongs at the owning resource root
```

## How to fix it

Move the folder or file to where its kind belongs: kind folders beside the role folder rather than
inside it, owner files and `controller.ts` at the resource root, subscribers into the subscriber
folder under a name that says what they do, helpers into `function/<name>.function.ts` files, and
unit tests of a module area below the modules namespace of the unit root.

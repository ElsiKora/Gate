# `nestjs/typeorm-lock-owner`

Every TypeORM row lock of a read takes its options from the one lock owner.

## What it checks and why

A hand-written TypeORM lock names no table, so PostgreSQL locks every row the statement reads: a
find whose `where` reaches a relation also locks the joined row, and a paged read over a join locks
rows past the page. A project that builds every lock clause in one owner function, which names the
locked tables from entity metadata, needs every read to take its lock from that function, with the
repository the read runs on, and every query builder to take it through the one call that maps those
tables to aliases. This rule holds the project to that.

It reads every tracked `.ts` file below `roots` (declaration files and the folders of
`[languages.typescript] skip-folders` left out).

**The owner.** `owner` must be a tracked `.ts` file below `roots` with exactly one value export, a
function: a function declaration or a `const` bound to an arrow or function expression, named or
default. Type exports do not count. A call is an owner call when its callee is an identifier (or a
member of a namespace import) that the file's scopes bind to an import, and that import, resolved
through the project's module resolution (relative paths and the tsconfig's `paths`) and followed
through re-exports and barrels, is the owner's function. A local function, a parameter or any other
binding of the same name is not the owner.

**Reads.** A find-options object is a read's when it is an argument, directly or through
parentheses, `as`, `satisfies`, `!`, conditional branches and spreads into an outer object literal,
of one of these calls:

| Read | Its repository |
| --- | --- |
| `R.<m>(options)`, `<m>` one of `read-methods` | `R` |
| `M.<m>(E, options)`, `<m>` one of `read-methods` | `M.getRepository(E)`; `E` must be an identifier |
| `<chain>.setFindOptions(options)` on a chain from `R.createQueryBuilder(alias)` or `M.createQueryBuilder(E, alias)`, through the chain's calls and one `const` | `R`, or `M.getRepository(E)` |
| `this.<m>(options)`, `<m>` one of `service-read-methods`, in a class that directly extends `service-base<E>` imported from `framework-package` | `this.<step-context-method><E>().getRepository(E)` |
| `this.<member>.<m>(options)`, `<m>` one of `service-read-methods` | the same, with the entity of the member's class when its written type names a class that directly extends `service-base<E>`; otherwise one identifier as both the type argument and the argument |

Only the method name and the argument's position make a call a read. `this.<m>(options)` in a class
that does not directly extend `service-base` is no read, and nothing in its options is checked.

**Sites.** A site is one `lock` property of a read's options; one `lock` property anywhere whose
value is an owner call; one `setLock` or `setOnLocked` call on a query builder (a chain from
`createQueryBuilder`, or a name whose written type is a query builder type of the `TypeORM`
package); one owner call that is not the value of a `lock` property; and, outside `owner`, one `mode`
property whose value is a lock value and that stands in no `lock` site. A `lock` key in any other
object is not read.

**Lock values.** A `mode` is a lock value when it folds to one of `lock-modes`: a string literal
(with or without `as const`), a `const` binding or an import of one, a member of an enum (followed
through imports and barrels), any branch of a conditional, or a name whose declaration has a written
type annotation naming an enum whose every member is one of `lock-modes`. A `lock` whose `mode`
folds to `optimistic` or `dirty_read` in every branch locks no row and is no site, unless
`lock-modes` lists that mode. A `mode` that folds to any other string, or that does not fold, is no
lock value, so its `lock` is a `lock-value` finding.

**Aspects.** Each site gets the first aspect that applies:

- `#lock-object`: the `lock` value is an object literal whose `mode` is a lock value, or (outside
  `owner`) an object literal has such a `mode` outside any `lock` site.
- `#lock-value`: a read's `lock` is not an owner call (each branch of a conditional must be an owner
  call or `undefined`), or an owner call is not the value of a `lock` property.
- `#receiver`: a `lock` holding an owner call is not in a read's options; the owner's first argument
  is not the read's repository; or the read's repository cannot be named (an entity-manager read
  whose entity is not an identifier, a `setFindOptions` on a chain without `createQueryBuilder`).
  Repositories are compared as syntax trees: parentheses and assertions are taken off, names are
  equal when they are bound to the same declaration, and a name bound by one `const` equals its
  initializer as well.
- `#row-list`: the owner's first argument is an array literal outside a builder's `setFindOptions`,
  an empty one, or one that names an element twice (compared as trees). The elements are not compared
  with the builder's repository.
- `#builder`: a `setLock` or `setOnLocked` call on a query builder (reported at the method name), or
  a builder's `setFindOptions` options with an owner `lock` and without `loadEagerRelations: false`.
- `#unreadable`: a file does not parse; that file is not read.
- `#owner`: `owner` is not a tracked TypeScript file below `roots` that exports exactly one function.
  Reported at the options file (at the configuration file when the options are inline); no other
  finding is reported.

A `lock` site's finding is at the `lock` property's first line, whatever lines the site spans; an
owner call's at the call; a `mode`'s at the property. Findings are reported in path and line order,
and two identical findings once. Each finding's key is the enclosing `Class.member` or named function
(`module` at the top level) and the site's ordinal in it, such as `BookService.expire#2`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `builder`, `lock-object`,
`lock-value`, `owner`, `receiver`, `row-list`, `unreadable`. Findings carry no target. The rule needs
the project's `owner`, so no preset turns it on.

### What it does not see

- A lock or `mode` built with `Object.assign`, a spread of a non-literal object, a computed key, or a
  mode read from configuration: `#lock-object` does not see it, but `#lock-value` still holds the
  read's `lock` to the owner.
- Options held in a variable and passed to a read (`repository.find(options)`): the object is not a
  read's argument, so its `lock` is read only when its value is an owner call (`#receiver`).
- A read whose method is not in `read-methods`, called through `call` or `apply`, a destructured
  method or an element access (`repository["find"]`).
- A query builder that is a parameter without a written `TypeORM` builder type, or that is reached
  through more than one `const`: its `setLock` is not seen, and its `setFindOptions` repository cannot
  be named.
- A declared type known only by inference, through generics, a type alias or a union; and a class
  member (`this.strength`) as a `mode`.
- A service member whose class extends the service base indirectly: only the step-context shape is
  checked.
- Raw SQL row locks, and files outside `roots`, such as tests.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `owner` | path | required | The one file that builds lock options. Without it the options are an error; leave the rule off instead. |
| `roots` | list of paths | `[layout.source-root]` | The folders whose tracked `.ts` files are read. |
| `framework-package` | string | absent: the layout's package (`layout.automator-package`) | The package of the service base class. |
| `service-base` | string | `"ApiServiceBase"` | The base class whose type argument names a service's entity. |
| `service-read-methods` | list of strings | `["get", "getList", "getMany"]` | The service methods whose options take the step-context form. |
| `step-context-method` | string | `"getApiFunctionStepContext"` | The service method whose result's `getRepository(<Entity>)` is a service read's repository. |
| `read-methods` | list of strings | `["count", "exists", "find", "findAndCount", "findOne", "findOneOrFail"]` | The repository and entity-manager methods whose argument is a find-options object. The `*By` methods take a `where`, in which `lock` is a column, so they are not listed. |
| `lock-modes` | list of strings | `["for_key_share", "for_no_key_update", "pessimistic_read", "pessimistic_write"]` | The lock modes that lock rows. |

Paths are relative to the project root and in normal form. The options may be inline or in an
options file:

```toml
[rules]
"nestjs/typeorm-lock-owner" = { level = "error", options = { owner = "src/lock/row-lock.ts" } }
```

## Example finding

```text
error nestjs/typeorm-lock-owner src/book/reader.ts:7
  src/book/reader.ts:7 builds a row lock object outside src/lock/row-lock.ts; take the lock from lockRows.
```

## How to fix it

Pass `lock: lockRows(<repository>, <strength>)` with the repository the read runs on; for a service
read, `this.getApiFunctionStepContext<Entity>().getRepository(Entity)` with the service's entity. Lock
a query builder through `.setFindOptions({ loadEagerRelations: false, lock: lockRows(…) })`, with a
list of the locked rows' repositories when it locks joined rows, and never call `setLock` or
`setOnLocked`.

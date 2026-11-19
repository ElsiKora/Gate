# `nestjs/typeorm-relation`

Every application-owned foreign key has persisted scalar columns and exactly one owning `TypeORM`
relation, typed through `Relation<Target>`, that declares its complete join and one semantic foreign
key name; no entity uses `@ForeignKey`, `@RelationId` or an inverse collection.

## What it checks and why

A foreign key that `TypeORM` derives on its own gets a hashed name, a join that guesses its columns,
and metadata that can disagree with the property type. Declaring every part explicitly keeps the
schema, the migrations and the TypeScript types in agreement, and the `Relation<Target>` wrapper keeps
the decorator metadata from reading a class an import cycle has not defined yet.

Every tracked `.ts` file below `layout.source-root` is read, declaration files included, in every
folder: the walk skips none of `[languages.typescript] skip-folders`. A decorator counts only when it resolves through the TypeScript binder to a runtime import of
`package`: by name, under an alias, or through a namespace import; a local function of the same name
is not one. An entity is a class with exactly one `@Entity(...)` call. A relation target resolves to
an exported entity of the same file, or through one runtime named import and explicit named re-exports
to exactly one exported entity. Import specifiers resolve relatively, or through the
`layout.path-aliases` entries `resolved-aliases` names, to `<base>.ts` or `<base>/index.ts` (exactly
one must exist); tsconfig is not read. A scalar type is `bigint`, `boolean`, `number`, `string`, an
intrinsic `Date` (when neither the file nor a global declaration declares a `Date` type), or an
exported repository enum, alone or with `| null`. A file without a syntax tree is not read;
`typescript/parse` reports it.

Findings are reported in the order of their messages. Each message starts with
`<path>:<line> <Class>.<member>`, the owning relation it is about.

Scope: project. Language: `typescript`. Reads history: no.

## Aspects

| Aspect | Reported when |
| --- | --- |
| `column-declaration` | a column decorator uses no supported literal overload (`()`, `("type")`, `({...})`, `("type", {...})`), or its options are not closed literals (`name`, `nullable`, `primary`) |
| `foreign-key-name` | the first join item's `foreignKeyConstraintName` is a `TypeORM` hash, does not match the pattern of `foreign-key-case`, or is not shorter than `max-foreign-key-bytes` UTF-8 bytes |
| `foreign-key-reuse` | two owning relations declare the same foreign key name; reported at the first |
| `join-column` | `@JoinColumn` does not receive one inline object or a non-empty inline array of them with `name`, `referencedColumnName` and, on the first item only, `foreignKeyConstraintName`; or a component repeats |
| `join-column-name` | the relation does not join through its own persisted `<member>Id` column, or its role qualifier prefixes none of its local names |
| `join-column-reference` | a local or referenced name does not resolve to exactly one persisted column |
| `join-column-type` | a join column has no closed scalar type, or the two sides' types differ |
| `nullability-exception` | a relation listed in `nullable-relations` departs from its entry |
| `prohibited-decorator` | a `banned-decorators` decorator is used |
| `relation-name` | the sole relation to a target carries a `mutable-roles` prefix instead of the target's resource name |
| `relation-nullability` | the `nullable` option disagrees with the property type, or `SET NULL` meets a non-nullable relation or local column |
| `relation-options` | the options are not one closed inline object with `nullable`, `onDelete` and `onUpdate` literals from the allowed values |
| `relation-owner` | a member has a `@JoinColumn` without an owning relation, or not exactly one owning relation and one `@JoinColumn` |
| `relation-target` | the target is not a direct zero-argument arrow to an identifier that resolves to an exported `*Entity` class |
| `relation-type` | the property is not typed `Relation<Target>` or `Relation<Target> \| null` |
| `relation-wrapper` | the property spells the target without the unaliased `Relation` imported by name from `package` |

Only the option/property mismatch of `relation-nullability` on a relation that meets its
`nullable-relations` entry carries a target, `<path>:<Class>.<member>`, so that an exception can
accept it; a relation that departs from its entry reports the mismatch without a target, which no
exception accepts:

```toml
[[exception]]
rule = "nestjs/typeorm-relation#relation-nullability"
target = "src/modules/shelf/shelf.entity.ts:ShelfEntity.rootShelf"
reason = "a self relation inserted before its root exists"
```

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `resolved-aliases` | array of string | `["@modules"]` | The `layout.path-aliases` keys specifiers resolve through; each must be defined there. |
| `foreign-key-case` | `"upper"` or `"lower"` | none | The spelling of a foreign key name: `FK_` or `fk_` followed by two or more lower-case words joined by `_`. Without it no name is checked, and a relation whose join reaches its foreign key name stops the run with an error. |
| `foreign-key-label` | string | none | The word the name message prints after the pattern; required with `foreign-key-case`. |
| `package` | string | absent: the layout's package (`layout.typeorm-package`) | The package the decorators and `Relation` come from. |
| `banned-decorators` | array of string | `["ForeignKey", "ManyToMany", "OneToMany", "RelationId"]` | Decorators no entity may use. |
| `column-decorators` | array of string | `["Column", "CreateDateColumn", "DeleteDateColumn", "PrimaryColumn", "PrimaryGeneratedColumn", "UpdateDateColumn", "VersionColumn"]` | Decorators that declare a persisted column. |
| `owning-relations` | array of string | `["ManyToOne", "OneToOne"]` | Decorators that declare an owning relation. |
| `relation-actions` | array of string | `["CASCADE", "NO ACTION", "RESTRICT", "SET NULL"]` | The values of `onDelete` and `onUpdate`. |
| `relation-deferrability` | array of string | `["INITIALLY DEFERRED", "INITIALLY IMMEDIATE"]` | The values of `deferrable`. |
| `mutable-roles` | array of string | `["active", "current", "default", "selected"]` | Role prefixes a sole relation must not carry. |
| `max-foreign-key-bytes` | integer, at least 1 | `63` | A foreign key name must be shorter than this many UTF-8 bytes. |
| `nullable-relations` | array of objects | `[]` | Relations whose `nullable: true` metadata stands with a non-null property and join, as an insertion cycle needs. |

Each `nullable-relations` entry has `path`, `class`, `member` (the relation), `target-path`,
`target-class` (its target entity), `local-name`, `referenced-name` (its one join component),
`scalar` (`"bigint"`, `"boolean"`, `"number"` or `"string"`, the type of both components),
`on-delete`, `on-update`, and optionally `reason`, which the departure message prints. The relation
must target exactly that entity with `nullable: true`, those actions and no `deferrable`, a non-null
property, and one join item of those names whose two columns are non-nullable and of that scalar type.

```toml
[rules]
"nestjs/typeorm-relation" = { level = "error", options = { foreign-key-case = "upper", foreign-key-label = "LIBRARY" } }
```

## Example finding

```text
error nestjs/typeorm-relation#foreign-key-name src/modules/book/book.entity.ts:21
  src/modules/book/book.entity.ts:21 BookEntity.author foreign key name "book_author" must match /^FK_[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/u in LIBRARY mode
```

## How to fix it

Declare each foreign key on one owning relation:

```ts
@JoinColumn({ foreignKeyConstraintName: "FK_book_author", name: "authorId", referencedColumnName: "id" })
@ManyToOne(() => AuthorEntity, { nullable: false, onDelete: "RESTRICT", onUpdate: "NO ACTION" })
author!: Relation<AuthorEntity>;
```

with a persisted `authorId` column of the same scalar type as `AuthorEntity.id`, and `Relation`
imported by name from the package.

# `nestjs/field-inventory`

Every field that can hold free text declares one class, its decision and its exposure.

## What it checks and why

Free text is where personal data, staff notes and third-party values end up, and where a field's
meaning drifts unnoticed. Every persisted text column and every API DTO property that can hold free
text is therefore classified once in `fields`: its class, the decision that owns it and the planes it is
exposed on. The rule derives the planes from the source, so an entry that no longer tells the truth is
reported, and so is a field that nobody classified.

The sources are the `.ts` files below `layout.source-root` (not `.d.ts`), outside the folders of
`[languages.typescript] skip-folders`.

- A field is an entity column of one of `text-column-types` (`@Column` or `@PrimaryColumn` of
  `layout.typeorm-package`; a column with no type and a `string` TypeScript type is a `varchar`), or a
  DTO property decorated with an `ApiProperty…` decorator of `layout.automator-package`.
- A field holds no free text when its TypeScript type is an enum or a string literal, its Automator type
  is no `STRING` or `OBJECT`, its Automator format is one of `machine-formats`, or it is an object of a
  nested DTO. A format of `code-formats` makes it a code of that kind, which needs no entry.
- A plane is a folder of `layout.module-planes` other than `layout.domain-folder` below
  `layout.modules-root`, named after its plane (`integration`, `internal`, `public`). A DTO reaches the
  plane of its file, of the controllers and custom routes that name it and of the DTOs that nest it; an
  entity column reaches the planes of the DTOs that carry or copy it and of the generated controllers
  whose DTOs and filters carry it, through mapped types (`PickType`, `OmitType`, `PartialType`,
  `IntersectionType`, matched by name), relations and table inheritance. Base classes of an entity are
  followed `base-depth` deep, DTO references `reference-depth` deep.

The aspects:

- `#unclassified`: a field that can hold free text has no entry. Reported at the field; the target pins
  the planes it reaches.
- `#exposure`: an entry's `exposure` differs from the planes the field reaches.
- `#private-exposure`: a `STAFF_NOTE` or `THIRD_PARTY` field reaches a plane other than `INTERNAL`
  without an `exposing-decision`, or a `STABLE_KEY` code reaches `PUBLIC`.
- `#classification`: an entry repeats another, a `CODE` has no `kind` or another class has one, a
  `MACHINE` value has no `reason`, a decision is not accepted, an `exposing-decision` is on a class that
  is not private, the `alternative` is the entry's own class, a plane is listed twice, or `role` and
  `scope` do not fit (only a `CODE` names them, an entity `CODE` needs a role and a DTO `CODE` has none,
  an `IDENTITY` code needs its scope and only it has one).
- `#stale-entry`: an entry matches no field, classifies a field that holds no free text or that a code
  format already makes a code, or carries an `exposing-decision` that exposes nothing.
- `#code-format`: a `CODE` field's Automator `pattern` is not exactly `/<grammar>/` of its kind.
- `#code-immutable`: a generated or named `UPDATE` or `PARTIAL_UPDATE` body can change an `IDENTITY`
  code.
- `#code-unique`: an entity that carries an `IDENTITY` code has no unique index, unique constraint or
  primary key over exactly its scope and the code.
- `#config-key-unreferenced`: a `CONFIG_KEY` code lists no `reference`.
- `#config-key-source`: a reference repeats another, cites a decision that is not accepted, has no
  reason, or does not name a string property `<path>: <Type>.<member>[.<member>…]` of an interface or
  object type alias in a file below one of `contract-roots`.
- `#config-key-field`: an entry that is no `CONFIG_KEY` code lists references.
- `#unreadable`: the rule cannot read source where it decides a field's scope or exposure, such as a
  controller configuration built by a call.

Findings about an entry are reported in the options file at the entry's line, or at `gate.toml` without
a line when the options sit there. Findings are sorted by path, line and message, and two identical
findings are reported once.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `classification`, `code-format`,
`code-immutable`, `code-unique`, `config-key-field`, `config-key-source`, `config-key-unreferenced`,
`exposure`, `private-exposure`, `stale-entry`, `unclassified`, `unreadable`. Targets:
`<path>:<Class>.<member>`, and `<path>:<Class>.<member> [<PLANE>,…]` (sorted, `[]` for none) for
`#unclassified`; `#unreadable` findings carry none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `fields` | list of entries | `[]` | The classified fields (below). |
| `contract-roots` | list of path prefixes | `layout.source-root` + `/config/runtime-config/`, `layout.infrastructure-folder` + `/configuration/`, `layout.cdk-root` + `/lib/config/` | Where configuration and secret contracts live. |
| `text-column-types` | list of strings | `bpchar`, `char`, `character`, `character varying`, `citext`, `hstore`, `json`, `jsonb`, `longtext`, `mediumtext`, `nchar`, `nvarchar`, `simple-array`, `simple-json`, `string`, `text`, `tinytext`, `varchar`, `xml` | Column types that hold text or JSON. |
| `code-formats` | map of format to code kind | `COUNTRY_CODE_ALPHA2`: `ISO_3166_ALPHA2`, `CURRENCY_CODE`: `ISO_4217`, `LANGUAGE_CODE_ISO639` and `LOCALE`: `ISO_639_1` | Automator formats that make a field a code. |
| `machine-formats` | list of strings | the 38 Automator formats that admit no free text, from `API_KEY` to `UUID` | Formats that make a field no text. |
| `base-depth` | integer | `8` | How many bases beyond an entity's direct base are followed. |
| `reference-depth` | integer | `12` | How deep DTO references are followed through constants. |
| `decisions` | path | none | The Markdown decision index that lists the accepted decisions. Entries need it: without it, or when the path is not tracked, the rule reports one `#classification` finding at the options and checks no decision. |

An entry of `fields` (a key the table does not list is a configuration error; the messages spell
`exposing-decision` as `exposingDecision`):

| Key | Required | Meaning |
| --- | --- | --- |
| `path`, `class-name`, `member` | yes | The field: its file, its class and its property. |
| `class` | yes | `PROPER_NAME`, `AUTHORED_CONTENT`, `PLAYER_CONTENT`, `PERSONAL_DATA`, `STAFF_NOTE`, `THIRD_PARTY`, `CODE` or `MACHINE`. |
| `decision` | yes | The accepted decision that owns the classification. |
| `exposure` | yes | The planes: `INTEGRATION`, `INTERNAL`, `PUBLIC`. |
| `kind` | for `CODE` | `UPPER_SNAKE`, `KEBAB`, `ISO_4217`, `MARKET_TICKER`, `ISO_3166_ALPHA2`, `ISO_639_1`, `CONFIG_KEY` or `STABLE_KEY`. |
| `reason` | for `MACHINE` | Why the value is no text. |
| `exposing-decision` | no | The accepted decision that exposes a `STAFF_NOTE` or `THIRD_PARTY` field beyond `INTERNAL`. |
| `alternative` | no | Another class the owner may still choose. |
| `reference` | for `CONFIG_KEY` | The contracts that reference the key: `[{ source, decision, reason }]`, all three required. |
| `role` | for an entity `CODE` | `IDENTITY` or `REFERENCE`. |
| `scope` | for `IDENTITY` | The other columns of its unique constraint. |

```toml
[rules]
"nestjs/field-inventory" = { level = "error", options-file = "gate/options/nestjs/field-inventory.json" }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/field-inventory#unclassified src/modules/domain/user/entity/user.entity.ts:8
  UserEntity.bio can hold free text and has no entry in gate/options/nestjs/field-inventory.json
```

## How to fix it

Add an entry for the field with its class, decision and the planes the rule reports, or narrow the
field (an enum, a code format) so that it holds no free text. Keep codes on their grammar, out of update
bodies and under a unique constraint, and remove entries the source no longer has.

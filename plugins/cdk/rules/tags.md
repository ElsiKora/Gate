# `cdk/tags`

Tags are applied in `lib/naming/` only, with the standard's keys, each of them applied.

## What it checks and why

Tags find a resource's project, environment and owner in billing and in the console; applied from one
function, every stack carries the same set. The rule judges `Tags.of(x).add(key, …)`, `Tags`
imported from `aws-cdk-lib`, and `x.setTag(key, …)` in `bin/` and `lib/`.

- `place`: a tag is applied outside `lib/naming/`.
- `key`: a key is neither a standard key (`Project`, `Environment`, `ManagedBy`, `Application`,
  `Name`) nor declared in `extra-tags`.
- `required`: `lib/naming/` never applies one of the standard keys.

A key is read as a literal, through a `const` of the same file, or, when it is the key of a loop over
`Object.entries(map)` or `Object.keys(map)` (a `for … of` or a `forEach`), as each key of `map`, an
object literal of the same file with its spreads. A key that cannot be read this way, such as a key
of a map built at run time, gives no finding, and while `lib/naming/` applies such a key `required`
reports nothing: the rule cannot tell which keys it applies.

Scope: project. Reads history: no. Aspects: `place`, `key`, `required`. Language: TypeScript.

## Options

None of its own; it reads `extra-tags` from `[plugins.cdk] options`.

```toml
[plugins.cdk]
source = "ElsiKora/Gate:cdk@0.3.0"
options = { extra-tags = [{ name = "CostCenter", job = "the budget the resource is billed to" }] }
```

## Example finding

```text
error cdk/tags#required lib/naming
  lib/naming/ never applies the standard tag ManagedBy
```

## How to fix it

Apply every tag in the tag function of `lib/naming/` and call it from each stack; use the standard
keys, or declare a new key in `extra-tags` with its job.

# `typescript/string-enums`

String-literal variant contracts use enums.

## What it checks and why

A set of string variants written as a literal union, or repeated as raw strings, has no single owner: a
rename misses copies, and a reader cannot find every use of a variant. An enum names the set once and
every use refers to its members.

Every tracked `.ts` file (not `.d.ts`) of the project, outside `[languages.typescript] skip-folders`,
is read on its own: a name resolves to the declarations of that file only, and a type name the file
does not declare is a library type. A file below one of `test-roots` is a test.

- `#literal-union`: a union type of two or more string literal types (`undefined` and `null` aside,
  `Readonly` unwrapped) that is not a key argument of a library generic named in
  `key-type-arguments`. In a test it is `#test-literal-union` instead.
- `#raw-enum-value`: a string literal that equals the value of a string enum member of the same file,
  outside that enum and outside a key argument.
- `#raw-discriminant`: a property whose type is a string literal, when the property is named in
  `discriminant-names` and takes two or more distinct values in the file, or when it tells the variants
  of an object union apart. Reported at each value.
- `#keyof-typeof-alias`: `type T = keyof typeof E` where every member of `E` holds its own name as its
  value; `E` itself is the type.
- `#unsupported-namespace`: a namespace form the rule does not resolve, reported instead of guessed: an
  external module declaration, a global augmentation, a namespace outside the top level and namespace
  bodies, a name declared with different kinds in separate blocks of one namespace, a namespace merged
  with a `const enum`, a variable, an import alias or a default export, and, outside a test, a
  `keyof typeof` on a value whose type the rule cannot know.

`#raw-enum-value`, `#raw-discriminant` and `#keyof-typeof-alias` are off in a test.
Every finding is reported at its line; two identical findings on one line are reported once.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `keyof-typeof-alias`,
`literal-union`, `raw-discriminant`, `raw-enum-value`, `test-literal-union`, `unsupported-namespace`.
Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `discriminant-names` | list of strings | `["action", "kind", "mode", "semantic", "state"]` | Property names that are discriminants wherever they hold a string literal type. |
| `key-type-arguments` | map of type name to index | `{ Omit = 1, Pick = 1, Record = 0 }` | Library generics whose argument at the index is a key list. |
| `alias-depth` | integer, at least 1 | `16` | The longest `import N = …` chain followed to a namespace. |
| `test-roots` | list of folders | `["test"]` | The folders of the tests, relative to the project root; a folder that is empty, absolute or not normalized is a configuration error. |

```toml
[rules]
"typescript/string-enums" = { level = "error", options = { test-roots = ["test", "e2e"] } }
```

## Example finding

```text
error typescript/string-enums#literal-union src/modules/user/user.type.ts:1
  uses a literal union type; use an enum instead
```

## How to fix it

Declare an enum for the variants and use its members; refer to the enum type instead of a `keyof typeof`
alias; declare namespaces and enums in a form the rule resolves.

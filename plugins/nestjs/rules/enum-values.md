# `nestjs/enum-values`

String enum values follow the value pattern.

## What it checks and why

Enum values travel: they are stored, logged and sent to clients. One form for every value keeps them
recognizable and stops two spellings of one value from both reaching a database.

Every member of an enum declared in a `.ts` file (not `.d.ts`) below `layout.source-root`, outside
`[languages.typescript] skip-folders`, whose initializer is a string literal or a template literal without substitutions,
through `as`, `satisfies`, `!`, `<T>` and parentheses, must have a value that `value-pattern` matches.
Numeric, computed and implicit members are not checked. Reported at the member.

The finding's target is `<Enum>.<member>`: the enum's name, a dot and the member's name as written (an
identifier or a string's value; a computed name keeps its brackets, as `E.["x"]`). The target holds no
path, so one exception keeps the value of that member in every enum of that name. Keep a value that an
external party's specification fixes with an exception on its target.

A file that cannot be parsed is skipped; `typescript/parse` reports it.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none. Target: `<Enum>.<member>`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `value-pattern` | regular expression | `"^[A-Z][A-Z0-9]*(_[A-Z0-9]+)*$"` | The pattern every value must match; anchor it to match the whole value. |

```toml
# gate.toml
[rules]
"nestjs/enum-values" = { level = "error" }
```

```toml
# gate.exceptions.toml
[[exception]]
rule = "nestjs/enum-values"
target = "EProviderStatus.ok"
reason = "the provider's API sends lower-case statuses"
```

## Example finding

```text
error nestjs/enum-values src/modules/user/enum/state.enum.ts:2 EState.active
  src/modules/user/enum/state.enum.ts EState.active has the value "active", which is not UPPER_SNAKE; rename it, or keep a value an external party's specification fixes as a policy exception with its decision
```

## How to fix it

Rename the value to match the pattern, or keep a value an external specification fixes with an
exception on its target.

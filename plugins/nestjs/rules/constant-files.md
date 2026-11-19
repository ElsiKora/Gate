# `nestjs/constant-files`

Constant files expose one `UPPER_CASE` object bag without callable code.

## What it checks and why

A constant file holds data, read by name. When it also holds functions, several exports or computed
values, callers start to depend on code that lives in the wrong place. The rule reads every `.ts` file
below `layout.source-root` (declarations included, `[languages.typescript] skip-folders` skipped) whose path ends with
one of `file-suffixes`, and reports, in statement order and then for the bag:

- `#shape`: a statement that is not a `const` declaration, import or export of names (a directive
  counts); a `let` or `var` statement; an export of more than one declarator; a private constant after
  the exported bag; not exactly one exported name; an exported name not declared in the file; a bag that
  is not an object literal (an `Object.freeze` call of one argument may wrap it); a spread in the bag.
- `#naming`: a private backing constant, a bag key or a bag name that is not `UPPER_CASE` (an ASCII
  uppercase letter, then uppercase letters, digits or `_`); a bag name without `bag-suffix`.
- `#callable`: a constant whose value retains a function or class, directly, through a `const` or a
  declaration of the file, inside an array or object literal (methods and accessors included), a branch
  of a conditional or logical expression, the last operand of a comma, or the single argument of an
  `Object` method listed in `freeze-calls`.

`await using` counts as `const`. A file that cannot be parsed is left to `typescript/parse`.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `callable`, `naming`, `shape`.
Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `file-suffixes` | array of string | `[".constant.ts", "/constant.ts"]` | The endings of the constant files' paths. |
| `bag-suffix` | string | `"_CONSTANT"` | The suffix of the exported bag's name. |
| `freeze-calls` | array of string | `["freeze", "seal", "preventExtensions"]` | The `Object` methods read through when looking for callable code. Only `Object.freeze` may wrap the bag. |

```toml
[rules]
"nestjs/constant-files" = { level = "error" }
```

## Example finding

```ts
const LIMIT = 10;
export const USER_SETTINGS = { LIMIT };
```

```text
error nestjs/constant-files#naming src/modules/user/constant/user.constant.ts:2
  src/modules/user/constant/user.constant.ts exports USER_SETTINGS; exported constant bags must end with _CONSTANT
```

## How to fix it

Keep only `const` declarations: private `UPPER_CASE` backing constants first, then one exported object
bag named with the suffix. Move functions and classes to the code file that owns them.

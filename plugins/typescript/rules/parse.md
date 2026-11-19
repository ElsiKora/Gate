# `typescript/parse`

Every tracked TypeScript or JavaScript source file can be parsed into a syntax tree.

## What it checks and why

The rule parses each tracked file whose extension is one of `extensions` with oxc. It reports a file
that oxc cannot recover from, and a file that is not UTF-8 text. For such a file the TypeScript
library gives an empty syntax tree, so every other rule that reads syntax trees skips it. Without this
rule, a broken file would pass those rules silently.

- Extensions compare case-sensitively, as the TypeScript compiler reads them: `App.TS` is not a source
  file. Declaration files (`.d.ts`) are source files.
- Every tracked file of the project counts, in any directory; `[languages.typescript] skip-folders`
  does not apply to this rule.
- Errors oxc recovers from are not reported: the TypeScript compiler and the linter own them.
- The rule reads no tsconfig. A `.cjs` file parses as a CommonJS script, a `.mjs` file as a module.
- One finding per file, at oxc's first error.

Scope: project. Language: `typescript`. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `extensions` | array of string | `["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"]` | The extensions, without the dot, of the files that must parse. Each must be one of the default eight; any other is a configuration error. |

```toml
[rules]
"typescript/parse" = { level = "error", options = { extensions = ["ts", "tsx"] } }
```

## Example finding

`src/app.ts`:

```ts
export const a = 1;
const = ;
```

```text
error typescript/parse src/app.ts:2:7
  src/app.ts cannot be parsed, so the checks that read its syntax tree skip it: Unexpected token
  help: fix the syntax error so the file parses; if the file is not source code, rename it or leave
  its extension out of the extensions option
```

A file that is not UTF-8 text is reported at the file, without a line:
`media/clip.ts cannot be parsed, so the checks that read its syntax tree skip it: the file is not UTF-8 text`.

The finding's target is the file's path.

## How to fix it

Fix the syntax error so the file parses. If the file is not source code (a media file with a `.ts`
name, a template), rename it, or narrow `extensions` so its extension is not read.

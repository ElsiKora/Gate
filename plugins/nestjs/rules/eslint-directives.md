# `nestjs/eslint-directives`

`ESLint` disable directives are exact, justified next-line exceptions.

## What it checks and why

A file-wide `eslint-disable`, a block directive or a directive at the end of a line silences more
than the one line it is meant for, and keeps silencing after the code changes. A directive without a
reason leaves the next reader guessing; one in code the lint configuration ignores is dead.

The rule reads every file with one of `extensions` at the project root or below one of `roots`,
outside `[languages.typescript] skip-folders`. Each `//` or `/* */` comment that mentions
`eslint-disable` or `eslint-enable` gets the first of these findings that applies:

1. `#dead-test-code`: the file is test or declaration code the lint configuration ignores: its first
   path segment is in `ignored-test-roots`, a segment is in `ignored-test-segments`, its name matches
   `ignored-test-file`, or it is a `.d.ts` file.
2. `#next-line-only`: the comment is a block comment.
3. `#placement`: code precedes the comment on its line.
4. `#syntax`: the comment is not exactly
   `// eslint-disable-next-line <rule-id>[, <rule-id> ...] -- <reason>` (rule ids separated by `", "`,
   each matching `rule-id-pattern`; a reason that starts and ends with a non-space character), or a
   rule id repeats. `eslint-enable` and a plain `eslint-disable` always fail here.
5. `#placement`: the next line is empty, missing or another comment.

Scope: project. Languages: `typescript` (which covers `.js`, `.mjs` and `.cjs`). Reads history: no. Findings carry no target. No
lint configuration is read; the ignored test code is described by the options.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form. A folder that holds no file passes.
`roots` are prefixes matched against paths as text, so they keep a trailing `/` when one is
written: `src/` matches `src/book.ts` and not `srcgen/book.ts`.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `roots` | array of string | `["infrastructure/", "scripts/", "src/", "test/"]` | Path prefixes read, besides the project's root files. |
| `extensions` | array of string | `["cjs", "js", "jsx", "mjs", "ts", "tsx"]` | The extensions of the files read. |
| `ignored-test-roots` | array of string | `["test", "tests"]` | First path segments of ignored test code, each a single name. |
| `ignored-test-segments` | array of string | `["__tests__"]` | Path segments of ignored test code, each a single name. |
| `ignored-test-file` | string (regular expression) | `(?:^\|\.)(?:e2e-spec\|spec\|test)\.[cm]?[jt]sx?$` | File names of ignored test code. |
| `rule-id-pattern` | string (regular expression) | `^(?:@[a-z0-9_-]+/)?[a-z0-9_-]+(?:/[a-z0-9_-]+)*$` | What a rule id looks like. |

```toml
[rules]
"nestjs/eslint-directives" = { level = "error", options = { ignored-test-roots = ["test", "e2e"] } }
```

## Example finding

```text
error nestjs/eslint-directives#syntax src/modules/book/book.service.ts:3
  must use // eslint-disable-next-line <rule-id>[, <rule-id> ...] -- <nonempty reason>
```

## How to fix it

Put `// eslint-disable-next-line <rule-id> -- <why>` on its own line directly above the line it
excuses, naming each rule once; remove directives from code the lint configuration ignores.

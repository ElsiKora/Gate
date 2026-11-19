# `nestjs/prefix-nesting`

Sibling resource folders and same-kind files whose names share a leading run of words are nested
under a folder named after that prefix.

## What it checks and why

A shared prefix names a sub-area. Siblings such as `book-shelf/` and `book-cover/`, or
`loan-request-body.dto.ts` and `loan-request-query.dto.ts`, belong in a `book/` or `loan-request/`
folder, so the tree shows the area once instead of repeating it in every name.

The rule reads every tracked path below `layout.source-root`, of any extension, in every folder
(the walk skips none of `[languages.typescript] skip-folders`), and judges each folder strictly below the source root:

- **Resource folders.** A child folder that directly holds one of `resource-files` is a resource
  folder. Two or more resource folders sharing a prefix are a group. One is enough when a sibling
  resource folder is named exactly the prefix (`author/` beside `author-profile/`).
- **Same-kind files.** The `.ts` files of one kind (`<stem>.<kind>.ts`, declaration files left out,
  stems in `ignored-stems` left out). Two or more stems sharing a prefix are a group. One is enough
  when the folder `<folder>/<prefix>` already holds a file of that kind at any depth: the file
  belongs beside the others. A prefix equal to the folder's own name is not a group.

A prefix is a run of whole words, split at `word-separator`. Longer prefixes group first, and a name
joins one group only. The finding is located at the folder and names the count, the prefix and the
kind.

Scope: project. Language: `typescript`. Reads history: no. One aspect (`""`). Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `resource-files` | array of string | `["module.ts", "controller.ts"]` | The files whose direct presence makes a folder a resource folder. |
| `ignored-stems` | array of string | `["index"]` | File stems that never join a group. |
| `word-separator` | string, not empty | `"-"` | The separator between the words of a name. |

The rule reads `layout.source-root` from `[plugins.nestjs] options`.

```toml
[rules]
"nestjs/prefix-nesting" = { level = "error", options = { ignored-stems = ["index"] } }
```

## Example finding

```text
error nestjs/prefix-nesting src/library/dto
  src/library/dto has 2 .dto.ts files sharing "loan-request"; nest them under src/library/dto/loan-request
```

## How to fix it

Create the folder the message names and move the group into it, dropping the prefix from each name
(`loan-request-body.dto.ts` becomes `loan-request/body.dto.ts`).

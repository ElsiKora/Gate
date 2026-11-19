# `ledger/manifest`

The ledger manifest is well formed, its files and identifier families exist, and no file names a
forbidden ledger path.

## What it checks and why

The manifest is a JSON file that describes the ledger: `canonicalRoot`, the `canonicalFiles` below it,
the `idFamilies` (each a `pattern` and a `file` below the root or a `directory` of
Markdown files, optionally `unique`), and `forbiddenReferences` (each a `label` and path `parts`).
Without it checked, a renamed ledger file or a stale path to an old ledger goes unnoticed. The
manifest is the one list of what a complete ledger holds.

- `input`: the manifest is not tracked, reported at its path; the rule checks nothing else until it
  is. A project without its ledger fails the rule: create it, point the option at it, or turn the input off with `levels = { input = "off" }` and a `decision`.

- `manifest-shape`: the manifest is not JSON, lacks a required field, has a duplicate canonical file,
  a forbidden reference with empty `parts` or a `label` that is not a string, a family without
  exactly one string `file` or `directory`, or a pattern that does not compile. A pattern has ASCII
  `\d`, `\w` and `\b` and the `u` and `m` flags: `^` and `$` match at every line, and an escape of a
  character that is not pattern syntax is an error. Look-around, back-references and inline flag
  groups are not supported.
- `manifest-file`: a canonical file, a family file or a family directory that is not tracked. A path
  that leaves the project is reported at the manifest.
- `id-family`: a family whose pattern matches nothing outside fenced code. `record-unique`: a unique
  family with a repeated match.
- `forbidden-reference`: a tracked text file, other than the manifest, that contains a forbidden
  path, at its first occurrence. Text files are those with an extension of `text-extensions`, without
  an extension, or named by `text-file-prefixes`, outside `skip-folders`.

Scope: project. Reads history: no. Aspects: `forbidden-reference`, `id-family`, `input`, `manifest-file`,
`manifest-shape`, `record-unique`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `manifest` | string | `"docs/ledger/manifest.json"` | The manifest. |
| `skip-folders` | array of string | `[".git", ".next", ".turbo", "build", "cdk.out", "coverage", "dist", "node_modules"]` | Directory names the scan skips at any depth. |
| `text-extensions` | array of string | `.cjs .conf .env .hcl .inc .js .json .md .mdc .mjs .mk .py .rb .sh .sql .ts .tsx .toml .yaml .yml` | Extensions, with the dot, of the files the scan reads. |
| `text-file-prefixes` | array of string | `["Dockerfile"]` | File names the scan reads whatever their extension, alone or followed by `.<suffix>`. |

```toml
[rules]
"ledger/manifest" = { level = "error", options = { skip-folders = ["vendor"] } }
```

## Example finding

A manifest with `"forbiddenReferences": [{ "label": "old ledger", "parts": ["docs", "old-ledger"] }]`
and a `README.md` whose third line links `docs/old-ledger/a.md`:

```text
error ledger/manifest#forbidden-reference README.md:3
  README.md references forbidden ledger path: old ledger
```

## How to fix it

Point the reference at the current ledger, create or rename the file the manifest requires, or correct
the manifest.

# `ci/runner-pinned`

Jobs run on a versioned runner image, never a moving label.

## What it checks and why

`ubuntu-latest` moves to a new image when GitHub decides: the toolchain under a job changes with no
commit, and a check that passed yesterday fails today. A versioned label (`ubuntu-24.04`,
`macos-15`) changes only in a commit, as an action update does. The rule reads every workflow file
the `workflows` glob matches and reports each `runs-on` label that matches one of the `moving`
globs: a single label, an item of a list, or an item of a mapping's `labels`. An expression
(`${{ matrix.runner }}`) is not evaluated and passes. A workflow that is not valid YAML stops the
run with the parser's message.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |
| `moving` | list of globs | `["*-latest"]` | The runner labels that move to new images. A glob that does not compile is a configuration error. |

## Example finding

`.github/workflows/ci.yml`:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
```

```text
error ci/runner-pinned .github/workflows/ci.yml:3
  job `build` runs on `ubuntu-latest`, a label that moves to new images
```

The target is `<workflow path>:<job id>`; the key is `<job id>:<label>`.

## How to fix it

Name the image version: `runs-on: ubuntu-24.04`.

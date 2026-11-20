# `ci/node-setup`

actions/setup-node reads the version from a file, sets a cache and runs once per job.

## What it checks and why

A Node version written in a workflow drifts from the one the repository's developers use, and
differs between workflows. Without `cache`, every run downloads the packages again. Setting Node up
twice in one job is a sign of two ways of installing the package manager. The rule reads every
workflow file the `workflows` glob matches, and in each job every step that uses
`actions/setup-node` (any ref), and reports:

- `version-file`: the step has no `node-version-file` input;
- `cache`: the step has no `cache` input;
- `once`: a second such step in the same job.

The rule names an action, not a language: a repository without Node has no such step and no finding.
A workflow that is not valid YAML stops the run with the parser's message.

Scope: repository. Reads history: no. Aspects: `version-file`, `cache`, `once`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

## Example finding

`.github/workflows/ci.yml`:

```yaml
jobs:
  build:
    steps:
      - uses: actions/setup-node@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4.4.0
        with:
          node-version: 24
```

```text
error ci/node-setup#version-file .github/workflows/ci.yml:4
  job `build` sets up Node without node-version-file
error ci/node-setup#cache .github/workflows/ci.yml:4
  job `build` sets up Node without a cache
```

The target is `<workflow path>:<job id>`; the key is `<job id>:<n>`, the step's place among the
job's setup-node steps.

## How to fix it

Install the package manager in one step, then set up Node once with the version file and the cache:

```yaml
      - uses: actions/setup-node@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4.4.0
        with:
          node-version-file: .nvmrc
          cache: npm
```

# `ci/checkout-credentials`

Every checkout sets persist-credentials: false.

## What it checks and why

`actions/checkout` writes the job's token into the clone's git configuration unless told not to.
Every later step, every action it runs and any artifact of the workspace can then read it. The rule
reads every workflow file the `workflows` glob matches and reports each step that uses one of
`actions` (any ref) without `with: { persist-credentials: false }`, at the step. A job that must
push (a release commit and its tag) is an exception in `gate.exceptions.toml` with its reason. A
workflow that is not valid YAML stops the run with the parser's message.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |
| `actions` | list of strings | `["actions/checkout"]` | The checkout actions, `owner/repo` without a ref. |

## Example finding

`.github/workflows/ci.yml`:

```yaml
jobs:
  build:
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4.2.2
```

```text
error ci/checkout-credentials .github/workflows/ci.yml:4
  job `build` checks out with its credentials kept in the clone
```

The target is `<workflow path>:<job id>`; the key is `<job id>:<step number>`.

## How to fix it

```yaml
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4.2.2
        with:
          persist-credentials: false
```

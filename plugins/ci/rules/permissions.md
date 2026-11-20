# `ci/permissions`

Every job sets its own token permissions, and a workflow's own permissions only read.

## What it checks and why

The token a job gets can do what its `permissions` allow. Permissions set at the top of a workflow
apply to every job that sets none, so a write scope there reaches jobs that never needed it, and a job
without its own `permissions` takes whatever the workflow or the repository's default grants. The
rule reads every workflow file the `workflows` glob matches and reports:

- `workflow-write`: a scope set to `write` in the workflow's top-level `permissions`, at its line;
- `job-missing`: a job without `permissions`, at the job; a job that calls a reusable workflow
  counts, since its `permissions` cap the called workflow's;
- `shorthand`: `read-all` or `write-all`, in the workflow's or a job's `permissions`.

A top-level `permissions: {}` or one with read scopes passes, and so does no top-level
`permissions` when every job sets its own. A workflow that is not valid YAML stops the run with the
parser's message.

Scope: repository. Reads history: no. Aspects: `workflow-write`, `job-missing`, `shorthand`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

## Example finding

`.github/workflows/ci.yml`:

```yaml
permissions:
  contents: write
jobs:
  build:
    runs-on: ubuntu-24.04
```

```text
error ci/permissions#workflow-write .github/workflows/ci.yml:2
  the workflow's own permissions grant `contents: write` to every job
error ci/permissions#job-missing .github/workflows/ci.yml:5
  job `build` sets no permissions of its own
```

The target is `<workflow path>:<job id>` for a job's finding and `<workflow path>` for the
workflow's; the key is the scope, the job id, or `workflow`.

## How to fix it

Set the top-level `permissions` to `{}` and give each job the scopes it uses: `contents: read` for a
job that only reads the repository, `{}` for one that reads nothing, a write scope only on the job
that writes.

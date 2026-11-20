# `ci/secrets-in-steps`

Secrets are passed to the steps that use them, never to a whole workflow or job.

## What it checks and why

A variable in a workflow's or a job's `env` reaches every step under it, and every action those
steps run. A secret there is readable by code that never needed it. The rule reads every workflow
file the `workflows` glob matches and reports:

- `workflow-env`: a variable of the workflow's top-level `env` whose value is an expression
  (`${{ ... }}`) that reads `secrets.`; plain text such as `see secrets.md` is not one;
- `job-env`: the same in a job's `env`;
- `inherit`: `secrets: inherit` on a job that calls a reusable workflow, which passes it every
  secret of the repository.

A secret in a step's `env` or `with`, and a reusable-workflow call that lists the secrets it
passes, pass. A workflow that is not valid YAML stops the run with the parser's message.

Scope: repository. Reads history: no. Aspects: `workflow-env`, `job-env`, `inherit`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

## Example finding

`.github/workflows/scan.yml`:

```yaml
jobs:
  scan:
    env:
      SCAN_TOKEN: ${{ secrets.SCAN_TOKEN }}
```

```text
error ci/secrets-in-steps#job-env .github/workflows/scan.yml:4
  job `scan`'s env `SCAN_TOKEN` holds a secret every step sees
```

The target is `<workflow path>:<job id>` (the workflow's path for `workflow-env`); the key is the
variable, with the job id before it for `job-env`, or the job id for `inherit`.

## How to fix it

Move the variable to the step that needs it:

```yaml
    steps:
      - name: Scan
        run: ./scan
        env:
          SCAN_TOKEN: ${{ secrets.SCAN_TOKEN }}
```

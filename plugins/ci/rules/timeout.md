# `ci/timeout`

Every job that runs steps sets timeout-minutes.

## What it checks and why

A job without `timeout-minutes` may run for six hours, GitHub's limit, when a step hangs: a runner
held, a queue blocked, a bill. The rule reads every workflow file the `workflows` glob matches and
reports:

- `missing`: a job with `steps` and no `timeout-minutes`, at the job; a job that calls a reusable
  workflow cannot set one and is not asked to;
- `over-max`: a `timeout-minutes` above `max-minutes`, when that option is set, at the value. An
  expression is not evaluated and passes.

A workflow that is not valid YAML stops the run with the parser's message.

Scope: repository. Reads history: no. Aspects: `missing`, `over-max`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |
| `max-minutes` | integer | unset | The longest `timeout-minutes` a job may set; unset, any. |

```toml
[rules]
"ci/timeout" = { level = "error", options = { max-minutes = 60 } }
```

## Example finding

`.github/workflows/ci.yml`:

```yaml
jobs:
  test:
    runs-on: ubuntu-24.04
    steps:
      - run: make test
```

```text
error ci/timeout#missing .github/workflows/ci.yml:3
  job `test` sets no timeout-minutes
```

The target is `<workflow path>:<job id>`; the key is the job id.

## How to fix it

Set `timeout-minutes` a little above the job's usual time. In `[ci.github]`, each job's
`timeout-minutes` is required and `gate sync` writes it.

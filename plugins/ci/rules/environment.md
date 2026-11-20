# `ci/environment`

A job that can write outside the checks (a release, a deploy, a publish) runs in an environment.

## What it checks and why

An environment carries protection rules: required reviewers, the branches and tags that may use it,
secrets only it holds. A job whose token can push commits and tags, create deployments, mint an
OIDC token or publish packages should pass them. The rule reads every workflow file the `workflows`
glob matches and reports each job that grants `write` on one of `scopes` (or `write-all`) and sets
no `environment`, at its `permissions`. A job that calls a reusable workflow cannot set an
environment and is not reported; the called workflow's jobs are read where they are. A job that
must write without one is an exception with its reason. A workflow that is not valid YAML stops the
run with the parser's message.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |
| `scopes` | list of strings | `["contents", "deployments", "id-token", "packages"]` | The token scopes whose `write` puts a job behind an environment. |

## Example finding

`.github/workflows/release.yml`:

```yaml
jobs:
  release:
    permissions:
      contents: write
```

```text
error ci/environment .github/workflows/release.yml:4
  job `release` writes `contents` and runs in no environment
```

The target is `<workflow path>:<job id>`; the key is the job id.

## How to fix it

Add `environment: release` to the job and give the environment its protection rules in the
repository's settings: the tags that may release, and reviewers if a person approves each release.
In `[ci.github]`, a job takes `environment`.

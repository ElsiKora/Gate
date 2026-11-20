# `ci/concurrency`

Every workflow has a concurrency group, and a newer run cancels only a pull request's.

## What it checks and why

Without a `concurrency` group two runs of one ref race: two deploys of the same branch, two
releases of the same tag. With `cancel-in-progress: true`, a newer run cancels the one in progress,
which is right for a pull request, whose older commits no longer matter, and wrong for a push to the
default branch, a deploy or a publish, which must run to their end. The rule reads every workflow
file the `workflows` glob matches and reports:

- `missing`: a workflow without a top-level `concurrency` that names a `group` (a plain
  `concurrency: <group>` counts as one);
- `cancel`: `cancel-in-progress` set to the literal `true` in a workflow that runs on any event
  other than `pull_request`, `pull_request_target` or `merge_group`.

An expression in `cancel-in-progress` is not evaluated and passes: the workflow `gate sync` writes
cancels with `${{ github.event_name == 'pull_request' }}`. A workflow that is not valid YAML stops
the run with the parser's message.

Scope: repository. Reads history: no. Aspects: `missing`, `cancel`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

## Example finding

`.github/workflows/ci.yml`:

```yaml
on: [push, pull_request]
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
```

```text
error ci/concurrency#cancel .github/workflows/ci.yml:4
  cancel-in-progress is true, so a newer run also cancels a running `push` run
```

The target is the workflow's path.

## How to fix it

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```

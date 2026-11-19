# `ci/workflow-run-target`

`workflow_run` triggers name workflows that exist.

## What it checks and why

A workflow with an `on.workflow_run` trigger runs after the workflows it names. GitHub matches the
names against each workflow's `name:` and says nothing when none matches: the workflow never runs.
The rule reads every workflow file the `workflows` glob matches, collects their names (a workflow's
`name:`, or its path when it has none), and reports:

- each entry of `on.workflow_run.workflows` that is not one of those names, at the entry's line;
  a single name instead of a list counts as a list of one;
- a `workflow_run` trigger without `workflows`, at the trigger's line.

Names compare exactly, case included. A workflow that is not valid YAML stops the run with the
parser's message.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository, both the ones checked and the ones whose names count. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

```toml
[rules]
"ci/workflow-run-target" = "error"
```

## Example finding

`.github/workflows/deploy.yml`, with a workflow named `Build` beside it:

```yaml
on:
  workflow_run:
    workflows: [build]
    types: [completed]
```

```text
error ci/workflow-run-target .github/workflows/deploy.yml:3
  workflow_run names `build`, which no workflow defines
  help: name a workflow by its `name:` (or its path when it has none); GitHub never starts this
  workflow for a name that no workflow has
```

The finding's target is `<workflow path>:<name>`, or the workflow's path for a trigger without
`workflows`.

## How to fix it

Write the name exactly as the other workflow's `name:` says, or remove the entry if that workflow
is gone.

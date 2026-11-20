# `ci/names`

Every workflow, job and step has a name, and a workflow's file is its name in kebab case.

## What it checks and why

GitHub lists workflows by name, branch protection requires checks by job name, and a run's log lists
steps by name. A missing name shows a path or a command instead; two jobs of one name are two checks
branch protection cannot tell apart. The rule reads every workflow file the `workflows` glob matches
and reports:

- `workflow`: a workflow without a top-level `name`;
- `file`: a file whose name (without `.yml` or `.yaml`) is not the workflow's name in kebab case:
  its runs of letters and digits, lowercase, joined by `-` (`Release And Publish` is
  `release-and-publish.yml`);
- `job`: a job without `name`, reusable-workflow calls included;
- `step`: a step without `name`;
- `duplicate`: a job named as an earlier job of the same workflow, or a workflow named as an earlier
  workflow;
- `job-id`: a job id that is not kebab case (lowercase letters and digits joined by single dashes).

A name that holds an expression (`Test ${{ matrix.os }}`) differs per run and is not checked for
`file` or `duplicate`. A workflow that is not valid YAML stops the run with the parser's message.

Scope: repository. Reads history: no. Aspects: `workflow`, `file`, `job`, `step`, `duplicate`,
`job-id`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

## Example finding

`.github/workflows/release.yml`:

```yaml
name: Release And Publish
jobs:
  build_image:
    steps:
      - run: make
```

```text
error ci/names#file .github/workflows/release.yml:1
  the workflow `Release And Publish` is in `release.yml`; its name in kebab case is `release-and-publish`
error ci/names#job-id .github/workflows/release.yml:3
  job id `build_image` is not kebab case
error ci/names#job .github/workflows/release.yml:4
  job `build_image` has no name
error ci/names#step .github/workflows/release.yml:5
  step 1 of job `build_image` has no name
```

The target is `<workflow path>:<job id>` for a job's or a step's finding and the workflow's path
otherwise; the key is the job id, `<job id>:<step number>` for a step, or the name for a duplicate
workflow.

## How to fix it

Name every workflow, job and step; rename the file or the workflow so one is the other in kebab case;
write job ids in kebab case. The name of a job branch protection requires stays when its steps
change.

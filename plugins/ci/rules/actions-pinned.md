# `ci/actions-pinned`

Workflows use actions and reusable workflows pinned by a full commit SHA.

## What it checks and why

A tag or a branch of an action is a pointer its owner can move: the next run of the workflow
executes whatever code it points to then, with the workflow's secrets and token. A full commit SHA
cannot move. The rule reads every workflow file the `workflows` glob matches, and in each job the
job-level `uses:` (a reusable workflow) and every step's `uses:` (an action). A reference passes
when it is:

- a local action or workflow: it starts with `./`;
- `<owner>/<repo>[/<path>]@<sha>`, where `<sha>` is 40 lowercase hexadecimal characters;
- `docker://<image>@sha256:<digest>`, where `<digest>` is 64 lowercase hexadecimal characters.

Anything else is reported at the line of the reference: a tag (`@v4`), a branch (`@main`), a short
or uppercase SHA, a Docker image by tag, an empty `uses:`. Every workflow counts, generated or
written by hand. A workflow that is not valid YAML stops the run with the parser's message.

Scope: repository. Reads history: no. Aspects: none.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |

```toml
[rules]
"ci/actions-pinned" = { level = "error", options = { workflows = ".github/workflows/*.yml" } }
```

## Example finding

`.github/workflows/ci.yml`:

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
```

```text
error ci/actions-pinned .github/workflows/ci.yml:5
  `actions/checkout@v4` is not pinned by a full commit SHA
  help: pin the action to the full 40-character commit SHA of the release you mean, with the tag in
  a comment (`uses: owner/action@<sha> # v4`), or a Docker image to its sha256 digest
```

The finding's target is `<workflow path>:<reference>`: `.github/workflows/ci.yml:actions/checkout@v4`.

## How to fix it

Look up the commit the tag points to (`git ls-remote https://github.com/actions/checkout v4`) and
write it with the tag as a comment:

```yaml
      - uses: actions/checkout@<40-character sha> # v4
```

A dependency bot that understands the comment keeps the pin current.

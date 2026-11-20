# `ci/actions-pinned`

Workflows use actions and reusable workflows pinned by a full commit SHA, with the release in a comment.

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

A SHA tells a reviewer nothing about which release it is. So a commit pin also carries the release, in
full, in a comment on its line: `uses: actions/checkout@<sha> # v4.2.2`. The aspect `version-comment`
reports a commit pin without a comment, or with one that does not match the `version-comment` option
(`# v4` names a major version, not a release). Local references and Docker digests need no comment.
The pin finding keeps the empty aspect, so its fingerprint is the one earlier releases gave.

Scope: repository. Reads history: no. Aspects: `version-comment` (and the pin finding, which has
none).

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `workflows` | string (glob) | `".github/workflows/*.{yml,yaml}"` | The workflow files, relative to the repository. `*` stays within a directory, `**` crosses directories, `{a,b}` is either; matching is case-sensitive. |
| `version-comment` | string (regular expression) | `"^v\\d+\\.\\d+\\.\\d+$"` | What the comment after a commit pin holds, without the `#` and trimmed. A pattern that does not compile is a configuration error. |

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
  help: pin the action to the full 40-character commit SHA of the release you mean, with the release
  in a comment (`uses: owner/action@<sha> # v4.2.2`), or a Docker image to its sha256 digest
```

```yaml
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4
```

```text
error ci/actions-pinned#version-comment .github/workflows/ci.yml:5
  `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1` names its release as `v4`, which does not
  match version-comment
```

The finding's target is `<workflow path>:<reference>`: `.github/workflows/ci.yml:actions/checkout@v4`.
A `version-comment` finding's key is the reference.

## How to fix it

Look up the commit the tag points to (`git ls-remote https://github.com/actions/checkout v4`) and
write it with the tag as a comment:

```yaml
      - uses: actions/checkout@<40-character sha> # v4.2.2
```

A dependency bot that understands the comment keeps the pin current.

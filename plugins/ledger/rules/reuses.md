# `ledger/reuses`

Every open implementation record names the shared mechanisms it reuses.

## What it checks and why

A catalog of shared mechanisms lists what already exists, so new work reuses it instead of building it
again. Each open implementation record says which catalog entries it reuses, or why none applies. The
rule is off until `catalog` is set, and reports nothing while `implementation` is not tracked.

- The catalog lists one mechanism per item ``- `<kebab-key>`: description`` with the fields, each once,
  `Decisions` (backticked accepted decisions) and either `Path` (backticked tracked paths; a directory
  ends with `/`) or `Planned by` (backticked open implementation records that will build it). Fenced code
  is ignored. Without a decision index, every decision passes.
- `catalog-entry`: a missing catalog, and one finding per malformed entry listing its problems.
- The records are the implementation records whose status is open (planned or started), outside the
  `exclude` directories. Each has exactly one field ``Reuses: `<key>`[, `<key>` …]`` or
  `Reuses: none; <reason>` with a reason of at least `reason-words` words that `generic-reasons` does
  not match.
- `reuses-missing`: no field, a repeated field, a key named twice, a vague reason, a malformed value.
  `reuses-unknown`: a key the catalog does not list. `reuses-planner`: a record that alone plans a
  mechanism names its key, or one of several planners does not.

Scope: project. Reads history: no. Aspects: `catalog-entry`, `reuses-missing`,
`reuses-planner`, `reuses-unknown`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `catalog` | string | `""` | The catalog of shared mechanisms; empty turns the rule off. |
| `exclude` | array of string | `[]` | Directories whose records the rule skips, with or without a trailing `/`. |
| `reason-words` | integer | `4` | The least number of words of a `none` reason. |
| `generic-reasons` | string | `(?i)^(?:not applicable\|n/?a\|none\|nothing\|no reuse\|nothing to reuse\|no shared mechanism\|no catalog entry applies)$` | A `none` reason this pattern matches is too generic. |

```toml
[rules]
"ledger/reuses" = { level = "error", options = { catalog = "docs/reference/shared-mechanisms.md" } }
```

## Example finding

```markdown
- `I-API-004`: Export
  - Status: `in progress`
  - Reuses: `csv-writer`
```

with no `csv-writer` in the catalog:

```text
error ledger/reuses#reuses-unknown docs/ledger/implementation-slices.md:3
  docs/ledger/implementation-slices.md I-API-004 reuses csv-writer, which docs/reference/shared-mechanisms.md does not list
```

## How to fix it

Name the catalog keys the work reuses, add the mechanism to the catalog, or say in a specific sentence
why no entry applies.

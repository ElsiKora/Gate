# `ledger/decision-counts`

The number of decisions and of superseded decisions in the managed block of the decision index.

## What it writes

The text between `<!-- gate:begin decision-counts -->` and `<!-- gate:end decision-counts -->` in
`decisions`:

```md
- Decision IDs: `<rows>`
- Superseded decisions: `<superseded>`
```

`<rows>` counts every line that starts with ``| `D-``, a repeated identifier included.
`<superseded>` counts the decisions whose status, read from their first row with at least four
cells, starts with `superseded`. The rest of the file is kept byte for byte.

Nothing is written while `decisions` is not tracked. Markers that are missing, repeated or reversed
are a finding of `gate/sync#decision-counts` at the first marker's line, and the file is left as it
is. The generator writes only `decisions`, so it owns no glob.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `decisions` | string | `"docs/ledger/decision-index.md"` | The decision index, relative to the project, in normal form. |

`generate` receives only the generator's own options, so a project whose `[plugins.ledger] options`
moves the decision index sets `decisions` here too.

```toml
[generate]
"ledger/decision-counts" = {}
```

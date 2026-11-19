# `ledger/closing-notes`

A temporary difference whose exact closer is closed carries a dated closing note, and one whose closer
is open carries none.

## What it checks and why

A record may list temporary differences in a field (`field`, such as `` `TEMPORARY_DELTA` records ``),
each naming the record that will close it. Once that record closes, the difference must say which
merge closed it; before, it must not claim to be closed. The records are the implementation records
and, with `parity`, the plan headings. `field` is required: enabled without it, the run stops with a
configuration error (exit 2).

- `input`: a file the rule reads that is not tracked (`[plugins.ledger] options` `implementation` and, with `parity`, the plan `parity.plan`), reported at
  its path; the rule checks nothing else until it is. Create it, point the option at it, or turn the input off with `levels = { input = "off" }` and a `decision`.

- The field is its line from the field name on, with the deeper lines below it up to a blank line or a
  line no deeper than the field line. A list item one level deeper starts a new piece; any other deeper
  line continues the piece above it.
- A piece splits into entries at each label `(1)` to `(99)`, optionally with one of
  `entry-label-letters` before the number, that starts the piece or follows `. `, `; ` or `: `. The text
  before the first label is an entry too, unless the field's value starts with `none`.
- An entry's closer is the first closing phrase (`exact closer`, `closer`, `closing slice`, `closing`,
  `closing in`, `closing exactly in`, `close in`, `closes in`, `closes only in`, `closed by`, or
  `closed for <product> by`, in any letter case), an optional colon and product name, and a backticked
  `I-` identifier. Products are `parity.products`. An unknown closer passes.
- The note is ``Closed (YYYY-MM-DD) by the merge of [<product> ]`I-…` as `<40-hex commit>` ``. The date
  is checked by its form only.
- A closer whose status is closed (its status or first word is one of `closed-statuses`) needs exactly
  one note; an open closer needs none.

Scope: project. Reads history: no. Aspects: `input`; the closing-note findings have no aspect.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `field` | string | required | The field that lists the differences; messages name its first backticked word. |
| `entry-label-letters` | array of string | `[]` | Single uppercase letters an entry label may carry before its number. |

```toml
[rules]
"ledger/closing-notes" = { level = "error", options = { field = "`TEMPORARY_DELTA` records", entry-label-letters = ["A", "B"] } }
```

## Example finding

```markdown
- `I-API-003`: Cache
  - Status: `planned`
  - `TEMPORARY_DELTA` records: (1) the cache stays local; closer `I-API-001`.
```

with `I-API-001` `` `completed` ``:

```text
error ledger/closing-notes docs/ledger/implementation-slices.md:3
  docs/ledger/implementation-slices.md I-API-003 has a TEMPORARY_DELTA entry whose exact closer I-API-001 is `completed`; add the note `Closed (YYYY-MM-DD) by the merge of `I-...` as `<full OID>`` naming the merge that closed it
```

## How to fix it

Add the one note naming the merge that closed the difference, remove a note whose closer is still
open, or split an entry that holds two differences at its labels.

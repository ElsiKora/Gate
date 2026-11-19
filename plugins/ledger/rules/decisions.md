# `ledger/decisions`

ADRs have their sections and linked decisions, and the decision index agrees with the ADR coverage.

## What it checks and why

The decision index (`[plugins.ledger] options` `decisions`) lists every decision with its status and
canonical file; the ADR coverage table (`coverage`) lists the same decisions with the ADRs that cover
them. When the two drift apart, a reader cannot tell which ADR explains a decision, or whether a
decision still holds. The rule reports nothing while `coverage` is not tracked.

- Rows of both tables start with `` | `D-``; each has four cells: the backticked identifier, the
  backticked status, the title (index) or a comma-separated list of backticked ADR file names
  (coverage), and the backticked canonical file. Fenced code and HTML comments are ignored.
- `record-shape`: a row without those four cells. `decision-status`: a status outside `statuses`.
  `record-unique`: an identifier listed twice in one table.
- `decision-coverage`: a decision only one table lists; a status or canonical file the tables
  disagree on; an ADR the coverage names that does not exist; an ADR that does not link the decision.
- `decision-canonical-file`: a canonical file, relative to `ledger-root`, that is not tracked.
- `adr-sections`: an ADR (`<adr-directory>/**/NNNN-*.md`, not `adr-template`) without exactly one
  line equal to each of `adr-sections`. `adr-linked-ids`: an ADR whose line starting with
  `linked-ids-prefix` names no backticked `D-` identifier.

Scope: project. Reads history: no. Aspects: `adr-linked-ids`, `adr-sections`,
`decision-canonical-file`, `decision-coverage`, `decision-status`, `record-shape`, `record-unique`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `ledger-root` | string | `"docs/ledger"` | The directory canonical files are relative to. |
| `adr-directory` | string | `"docs/adr"` | The directory of the ADRs. |
| `coverage` | string | `"docs/adr/decision-coverage.md"` | The ADR coverage table. |
| `adr-sections` | array of string | `["## Context", "## Decision", "## Consequences", "## Alternatives Considered", "## Verification"]` | The heading lines every ADR has once. |
| `statuses` | array of string | `["accepted", "deferred", "proposed", "rejected", "superseded"]` | The statuses a row may have. |
| `adr-template` | string | `"0000-template.md"` | The ADR template's file name, which is not an ADR. |
| `linked-ids-prefix` | string | `"- Linked ledger IDs:"` | The start of the ADR line that links decisions. |

```toml
[rules]
"ledger/decisions" = { level = "error", options = { adr-directory = "docs/decisions" } }
```

## Example finding

`docs/adr/decision-coverage.md`:

```markdown
| `D-API-001` | `accepted` | `0001-api.md` | `decisions/api.md` |
```

with `docs/ledger/decisions/api.md` untracked:

```text
error ledger/decisions#decision-canonical-file docs/adr/decision-coverage.md:1
  D-API-001 references missing canonical file decisions/api.md
```

## How to fix it

Make the index and the coverage list the same decisions with the same status and canonical file, add
the missing ADR sections or the `Linked ledger IDs` line, and create or rename the files the rows name.

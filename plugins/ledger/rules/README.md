# The `ledger` plugin

The ledger is a set of Markdown files that record a project's decisions and work: a decision index
and its ADRs, a plan, implementation, verification and follow-up records, and production blockers.
The rules of this plugin keep those records consistent with each other.

| Rule | What it holds true |
| --- | --- |
| [`ledger/decisions`](decisions.md) | ADRs have their sections and linked decisions, and the decision index agrees with the ADR coverage |
| [`ledger/manifest`](manifest.md) | The ledger manifest is well formed, its files and identifier families exist, and no file names a forbidden ledger path |
| [`ledger/records`](records.md) | Ledger records keep their fields, counterparts, commits, dependencies and closure evidence |
| [`ledger/record-status`](record-status.md) | A paired plan record, its implementation and its verification share one allowed status |
| [`ledger/blockers`](blockers.md) | A planned record names no closed blocker and a started record no blocker |
| [`ledger/closing-notes`](closing-notes.md) | A temporary difference whose exact closer is closed carries a dated closing note |
| [`ledger/reuses`](reuses.md) | Every open implementation record names the shared mechanisms it reuses |
| [`ledger/exception-owners`](exception-owners.md) | The owner an exception names is an open implementation record |
| [`ledger/exception-decisions`](exception-decisions.md) | The decision an exception names is an accepted decision of the index |
| [`ledger/rule-decisions`](rule-decisions.md) | The decision a rule entry names is an accepted decision of the index |

| Generator | What it writes |
| --- | --- |
| [`ledger/decision-counts`](../generators/decision-counts.md) | The decision and superseded counts in the managed block of the decision index |
| [`ledger/review-progress`](../generators/review-progress.md) | The review progress table in the managed block of a roadmap |

## Shared options

Every rule reads `[plugins.ledger] options`. Paths are relative to the project root; a ledger file
outside the project is read only when `[plugins.ledger] reads` grants it.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `decisions` | string | `"docs/ledger/decision-index.md"` | The decision index: a table whose rows start with a backticked `D-` identifier, then the status, the title and the file. |
| `implementation` | string | `"docs/ledger/implementation-slices.md"` | The entry file of the implementation records. |
| `verification` | string | `"docs/ledger/verification.md"` | The entry file of the verification records. |
| `follow-ups` | string | `"docs/ledger/follow-ups.md"` | The entry file of the follow-up records. |
| `blockers` | string | `"docs/ledger/production-blockers.md"` | The entry file of the production blocker records. |
| `parity` | table | absent | A plan whose headings pair with implementation and verification records; absent, the paired checks are off, and `ledger/record-status`, which needs it, stops the run with exit 2. |
| `closed-statuses` | array of string | `["closed", "complete", "complete (external prerequisite)", "completed", "done", "passed", "verified"]` | The statuses that close a record, compared with the status in lowercase. |
| `planned-prefixes` | array of string | `["planned"]` | A status that starts with one of these is planned work. |
| `started-prefixes` | array of string | `["in progress", "in-progress"]` | A status that starts with one of these is started work. |

`parity` has these keys, all required except `products`:

| Key | Meaning |
| --- | --- |
| `plan` | The entry file of the plan, whose `### I-<prefix>-…: title` headings are the paired records. |
| `prefix` | Uppercase letters and digits after `I-` and `V-` that mark a paired record, such as `CORE`. |
| `label` | The name of the paired work in messages. |
| `reference` | The plan field that holds the full commit id of the reference an active record follows. |
| `products` | Product names a closer or a closing note may put before a record identifier (default none). |
| `fields` | The other plan fields an active record carries, each exactly once. |
| `active-status` | The plan status that makes `reference` and `fields` required. |

```toml
[plugins.ledger]
source = "ElsiKora/Gate:ledger@0.2.0"
options = { parity = { plan = "docs/ledger/plans/core.md", prefix = "CORE", label = "Core", reference = "Upstream reference", fields = ["Parity evidence"], active-status = "in progress" } }
```

## Records

An entry file and every Markdown file below the directory of the same name (`implementation-slices.md`
and `implementation-slices/**/*.md`) form one collection; fenced code is ignored. A record is a line
``- `I-…`: title`` (`V-`, `F-`, `PB-` alike), or `### I-<prefix>-…: title` in the plan, and runs to
the next top-level item (the next `### ` heading in the plan). Its fields are the lines
`  - Name: value` below it (`- **Name:** value` in the plan). Its status is the first backticked value
of its first `Status` field. The first definition of an identifier wins.

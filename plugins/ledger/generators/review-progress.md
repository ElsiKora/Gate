# `ledger/review-progress`

The review progress table in the managed block of a roadmap.

## What it reads

The roadmap at `path`, outside fenced code:

- the feature inventory: each `#### F<nnn>: <title>` heading and its `` - Review: `<status>` `` line;
- the entries of the decision journal (`###` headings under `journal-heading`), each with one
  `Counts: <n> answer(s) · <kind>` line right below the heading, where `<kind>` is a feature
  (exactly `F` and three digits) or one of the configured kinds;
- the rows of the execution queue (`| <n> | … | `I-…` |` under `queue-heading`), whose
  implementation records are read from `implementation` (the entry file and the Markdown files of
  its same-stem directory) as the ledger rules read them;
- the decision index at `decisions`, counted as `ledger/decision-counts` counts it.

## What it writes

The text between `<!-- gate:begin review-progress -->` and `<!-- gate:end review-progress -->`: a
`Metric` / `Count` table, aligned the way Prettier prints a table while it fits in 100 columns and
compact beyond, with the rows

`Answered product choices`, `Feature answers`, one `<KIND> answers` per cross-cutting kind in option
order, `Features with completed product review`, `Features in progress`, `Unreviewed features`,
`Total inventory features`, `Features remaining to finish, including the active feature`,
`Execution I/V pairs`, `Completed I/V pairs` (status `completed` or `done`), `In-progress I/V pairs`
(any other status), `Planned I/V pairs` (status `planned`), `Decision IDs` and `Superseded
decisions`.

What the roadmap does not say this way is a finding of `gate/sync#review-progress` at its line, and
the table is still written from what could be read: a `Counts:` line outside a journal entry, after
other text, repeated, not in the form above, or with `0 answers` for a kind that answers (or
another count for a zero-answer kind); an entry without a `Counts:` line or naming a feature not in
the inventory; a feature listed twice, without a review status, with a second one or an unknown one;
a queue row whose record is missing or has no status (at the row's line). An untracked roadmap or
decision index is a finding (the index's counts are then written as 0); missing or
misplaced markers are a finding at the first marker's line. The generator writes only `path`, so it
owns no glob.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `path` | string | required | The roadmap, relative to the project, in normal form. |
| `decisions` | string | `"docs/ledger/decision-index.md"` | The decision index. |
| `implementation` | string | `"docs/ledger/implementation-slices.md"` | The entry file of the implementation records. |
| `cross-cutting-kinds` | array of string | required | The answer kinds besides features, each uppercase letters and digits, not of the feature form `F` and three digits, and listed once across both lists. |
| `zero-answer-kinds` | array of string | required | The kinds that record no answer: only they say `0 answers`, and always. |
| `journal-heading` | string | `"## Decision Journal"` | The heading of the decision journal. |
| `queue-heading` | string | `"## Accepted Execution Queue"` | The heading of the execution queue. |
| `completed-reviews` | array of string | `["retain existing", "implement", "deferred", "rejected"]` | Review statuses of a completed review. |
| `in-progress-reviews` | array of string | `["awaiting answer", "clarification needed"]` | Review statuses of a feature in review. |
| `unreviewed-review` | string | `"unreviewed"` | The review status of a feature not yet reviewed. |

`generate` receives only the generator's own options, so a project whose `[plugins.ledger] options`
moves the decision index or the implementation records sets `decisions` or `implementation` here
too.

```toml
[generate]
"ledger/review-progress" = { path = "docs/ledger/plans/roadmap.md", cross-cutting-kinds = ["RET"], zero-answer-kinds = ["PROCESS"] }
```

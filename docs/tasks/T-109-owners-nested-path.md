# T-109: the `ledger` owners rules read the implementation records from the repository root

| Field | Value |
| --- | --- |
| Status | in review |
| Release | the next `ledger` plugin release after 0.3.2 |
| Writer | session "Gate T-109" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | T-105 merged |
| Touches | `plugins/ledger/` (the owners rules, their pages, CHANGELOG), the core's listing of exceptions (`crates/gate-host`, the doc comments of `wit/gate.wit` and the SDK), tests, `CHANGELOG.md`, `docs/`, this task file |

## Goal

`ledger/exception-owners` and `ledger/root-exception-owners` read the implementation records
relative to their scope root. In a nested project that inherits the root's `[plugins.ledger]`
options, they read a path inside the nested folder, so they find no records and judge the nested
project's exceptions wrongly. T-105 left this out of its scope.

## Required outcome

1. The records path in the ledger options means one place in the repository, wherever the options
   are written, as the decision index does since T-105.
2. Each exception of every `gate.exceptions.toml` in the workspace is judged once, by one rule, at
   its file and line.
3. The plugin interface stays `gate:plugin@0.3.0`. If the outcome needs a change to it, stop and
   report to the lead before writing code.
4. No alias of an old rule id. If rules merge or are renamed, the CHANGELOG says so.

## First step: the design, to the lead

Before writing code, send the lead a short design. It should answer:

- which call judges a nested project's exceptions;
- where the records are read from;
- whether the two rules become one, as the decision rules did in T-105.

## Proof

- Synthetic fixtures with a root project and a nested project, each with exceptions that name
  owners:
  - valid owners pass;
  - an unknown owner gives exactly one finding, at the right file and line;
  - a root that is not a project still works;
  - no exception is reported twice.
- The rules' pages and CHANGELOG match.
- All cargo runs go through the lane.

## Design (accepted by the lead, way A)

- **Which call.** The repository call judges every exception of every `gate.exceptions.toml` once,
  at its `[[exception]]` line, whether or not the root is a project. Project calls judge none.
- **Where the records are read.** The repository call's `[plugins.ledger] options implementation`,
  relative to the repository root, with the whole tree granted: the place of the decision index since
  T-105. No new option, no `reads`.
- **One rule.** `ledger/exception-owners` becomes repository-scope; `ledger/root-exception-owners`
  and the rule's own `exceptions` option are gone, with no alias. The presets turn the rule on at the
  level the two had.
- **What tells the rule the files.** In `gate:plugin@0.3.0` the repository call listed an exception
  in `rule-decisions` only when it named a decision, and the plugin cannot rebuild the set of files
  the core reads (it does not see `[workspace] ignore`). The WIT types are unchanged; their doc
  comments now say that every exception is listed, with an empty `decision` when it names none,
  while rule entries, families and profile rules are listed only when they name one.
  `ledger/config-decisions` skips an empty decision; `ledger/exception-owners` reads the
  `gate.exceptions.toml` files that list names.

## Cross pairs

A script kept outside the repository runs `gate check` on a synthetic root
project `top` with a nested project `infra/cdk`, root records listing `I-OPEN-001` planned and
`I-DONE-002` completed. The root exceptions name `I-OPEN-001` (line 1) and `I-DONE-002` (line 7),
neither with a decision. The nested exceptions name `I-OPEN-001` with an accepted decision (line 1)
and `I-NOPE-003` with none (line 8); the nested folder has records of its own that list
`I-NOPE-003`. A fresh cache each run. ledger 0.3.2 is built from `plugin-ledger-v0.3.2` (sha256
`b6815c8b...`, the bytes T-108 recorded); core 0.3.3 is the released binary.

| Core | Ledger | Exit | Findings |
| --- | --- | --- | --- |
| 0.3.3 | 0.3.2 | 1 | `gate.exceptions.toml:7` (completed); the nested `I-NOPE-003` passes against the nested folder's records |
| 0.3.3 | this branch | 1 | `infra/cdk/gate.exceptions.toml:8` (unlisted) only; the root file has no decided exception, so it is not listed and its completed owner is missed |
| this branch | 0.3.2 | 1 | 3 `ledger/config-decisions` findings, "names the decision , which ... does not list", one per undecided exception, plus 0.3.2's `gate.exceptions.toml:7` |
| this branch | this branch | 1 | `gate.exceptions.toml:7` (completed) and `infra/cdk/gate.exceptions.toml:8` (unlisted), each once |

## Resume point

Written on `task/T-109-owners-nested-path`. Lane: clippy alone on `317e2504` exit 0. Next: the full
round on the head, push, report the head and the exit codes to the lead.

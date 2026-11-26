# T-109: the `ledger` owners rules read the implementation records from the repository root

| Field | Value |
| --- | --- |
| Status | open |
| Release | the next `ledger` plugin release after 0.3.2 |
| Writer | not assigned |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | T-105 merged |
| Touches | `plugins/ledger/` (the owners rules, their pages, CHANGELOG), tests, `docs/plan.md`, this task file |

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

## Resume point

Not started.

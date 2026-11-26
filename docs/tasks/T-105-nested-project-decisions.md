# T-105: decisions named in a nested project are checked against the workspace's index

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | the next `ledger` plugin release, and the core release if the core must hand more to the rule; the plugin interface stays `gate:plugin@0.3.0` |
| Writer | "Gate T-105" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `80db512` |
| Touches | `plugins/ledger/` (the decision rules, their pages, CHANGELOG), and the core only where it hands decisions to a rule; tests; `docs/plan.md`, this task file |

## Goal

A workspace has a root project and a nested project with its own `gate.toml`. The decision index is
at the repository root. Its options are in the root's `[plugins.ledger]`.

Today:

- `ledger/rule-decisions` (project scope) checks the decisions written in the project's own
  `gate.toml` against `[plugins.ledger] options decisions`.
- In the nested project the index path resolves relative to the nested project's folder, so it
  finds no index. Every decision named there, valid or not, is reported as "does not list". A path
  such as `../../docs/...` is accepted but reads nothing.
- `ledger/root-rule-decisions` reports nothing when the root is a project.

So a decision named in a nested project's `gate.toml` is checked by no rule. The first consumer
names 15 there and holds them by review until Gate checks them.

## Required outcome

1. Every decision named in any `gate.toml` of the workspace is checked against the workspace's
   decision index: in `[rules]` entries, in families, in `[[profiles]]` rules, and in exceptions
   that name a decision.
2. Each written decision is judged exactly once, by one rule.
3. A decision missing from the index, or not `accepted`, gives one finding at the entry's line in
   the file that wrote it.
4. The plugin interface stays `gate:plugin@0.3.0`. If the outcome cannot be reached without
   changing it, stop and report before writing code.
5. No alias of an old rule id and no second way to configure the index. If a rule is merged or
   renamed, its old id is gone and its page says so in the CHANGELOG only.

## First step: the design, to the lead

Before code, send the lead a short design that answers these:

- which rule judges a nested project's entries;
- where it reads the index;
- what the core already hands a repository-scope rule;
- what changes for a workspace whose root is not a project;
- how the exception rules (`ledger/exception-decisions`, `ledger/exception-owners` and their root
  forms) behave for a nested project's exceptions.

The lead answers within the session.

## Proof

- Synthetic fixtures with a root project and a nested project, both writing decisions:
  - valid ones pass;
  - a missing one and a `proposed` one each give exactly one finding, at the right file and line;
  - a root that is not a project still works;
  - no entry is reported twice.
- The rules' pages, examples and CHANGELOG match.
- All cargo runs go through the lane.

## Design (approved by the lead, 2026-11-26)

- One repository-scope rule, `ledger/config-decisions`, judges every decision written in the
  workspace's configuration. It replaces `ledger/rule-decisions`, `ledger/root-rule-decisions`,
  `ledger/exception-decisions` and `ledger/root-exception-decisions`; the old ids are gone, and the
  ledger presets name the new rule at the level the old ones had.
- The core's repository call carries, in `check-request.rule-decisions`, every decision written in
  the workspace, whether or not the root is a project: the rule entries, families and
  `[[profiles]]` rules of every `gate.toml`, and the `decision` of every exception of every
  `gate.exceptions.toml` (key: the exception's `rule`; at: its `[[exception]]` line). A project's
  call carries none. `config-file` is unchanged.
- The index is the root's `[plugins.ledger] options decisions`, relative to the repository root.
- The plugin interface stays `gate:plugin@0.3.0`: the WIT types are unchanged; the doc comments of
  `rule-decision`, `check-request.rule-decisions` and `check-request.config-file` state the new
  contract. Ledger 0.3.2 needs core 0.3.3 or later; with core 0.3.2 it leaves decisions in nested
  projects unjudged. Both CHANGELOGs say so.
- The rule is repository-scope, so the T-107 filter (a project call's findings on files a nested
  project owns are dropped) does not touch it.

## Left out

- `ledger/exception-owners` and `ledger/root-exception-owners` read the implementation records
  relative to their scope root, so in a nested project that inherits the root's ledger options they
  read a path inside the nested folder; that defect of the owners rules is a follow-up task.

## Resume point

Code, tests and documents written and committed on the branch; the lane round (fmt, clippy, the full
test, deny) runs on the committed tip when the lane frees. Next: read the round's summary, fix what fails, record the exit
codes here, push, and send the lead the head OID.

# T-054: The 0.1.0-to-0.2.0 baseline upgrade, written down

| Field | Value |
| --- | --- |
| Status | in review |
| Release | the lead sets it |
| Writer | "Gate 005" |
| Reviewer | the lead |
| Depends on | v0.2.0 |
| Touches | `docs/guide/configuration.md` (Baselines), `CHANGELOG.md` |

## Goal

A baseline written by 0.1.0 moves to 0.2.0 with `gate baseline prune`, which re-keys what it can.
Findings in or naming a nested project's option file changed path and message in 0.2.0 (T-031), so
their entries no longer match and prune removes them; the findings are reported again. Prune stays as
it is: matching 0.1.0's repeated paths and old messages would read an old format. The guide says how
to finish the upgrade: prune, check, pair each reported finding with an entry prune removed, and when
every finding pairs, write the baseline once.

Seen on a consumer's tree: every finding reported after prune paired one to one with an entry prune
removed.

## Done when

- The Baselines section of the configuration guide gives the upgrade and the check, generic.
- `CHANGELOG.md` has the entry under Unreleased.
- The lead has read it. Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: branch `task/T-054-upgrade-note` from main `105ca02`; guide paragraph, changelog entry and
this file written. Next: the lead's read.

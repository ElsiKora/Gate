# T-082: A 0.3.0 section in every plugin CHANGELOG

| Field | Value |
| --- | --- |
| Status | done; ready for the lead |
| Release | 0.3.0 |
| Writer | the T-066 writer |
| Reviewer | the lead |
| Depends on | `main` at `9eeb608` (T-081 merged) |
| Touches | `plugins/*/CHANGELOG.md`; documents only |

## Goal

Every plugin is released at 0.3.0 with the core, because each must be rebuilt for
`gate:plugin@0.3.0`. Each plugin CHANGELOG gets a `# 0.3.0 (unreleased)` section in the house form:

- the heading `# 0.3.0 (unreleased)`;
- the line "Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.",
  the form of the released sections;
- the plugin's real 0.3.0 changes, from `git log v0.2.1..main -- plugins/<plugin>`.

Released sections and versions are not touched.

## Done when

- `scripts/release-check.sh 0.3.0` reports `ok` for the top heading of all 13 plugin CHANGELOGs.
- A script shows each plugin CHANGELOG's released sections, from the first released heading on,
  byte-identical to `main`.
- The leak scan prints nothing. There is no code change, so there is no lane round.

## Resume point

2026-11-20T20:26:55+00:00: done, documents only.
- `cdk`, `docker`, `repository` and `typescript` had an `## Unreleased` heading. It is now
  `# 0.3.0 (unreleased)`, and the entries are unchanged.
- `cdk`, `typescript`, `frontend` and `git` had no built-for line. `docker`, `repository` and
  `nestjs` had the short form. All seven now have the full line.
- `ledger` and `markdown` were topped by their released 0.2.0. Each gets a new section.
  - `git log` since their tags shows no rule change. For `ledger`, one test passes the new
    `plugin-options` argument of `generate`. For `markdown`, nothing changed.
  - So each entry says the plugin is rebuilt for `gate:plugin@0.3.0`, and that its rules and
    output are unchanged. The `ledger` entry also names the `[plugins.ledger]` options that
    `generate` now receives.
- `agents`, `ci`, `docs` and `node` were already in the house form.
- `scripts/release-check.sh 0.3.0`: 14 top headings `ok` (the core's and 13 plugins'), exit 1.
  85 of 140 places disagree, down from 91 on `main`, because six headings now agree. The rest
  are the versions and pins left to the cut.
- The released-section comparison against `HEAD` printed "released sections identical: 13 of 13".

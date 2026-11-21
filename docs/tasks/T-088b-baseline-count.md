# T-088b: The summary counts the findings the baseline holds back

| Field | Value |
| --- | --- |
| Status | round queued |
| Release | v0.3.0 (core) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | `main` at `d24356fc` (after T-080 and T-088) |
| Touches | `crates/gate-host/src/findings.rs`, `crates/gate-host/src/report.rs`, `crates/gate-host/src/lib.rs`, `crates/gate-host/tests/host/findings.rs`, `crates/gate/tests/plugins.rs`, `docs/guide/configuration.md`, `CHANGELOG.md` |

## Goal

Minor finding 19 of the release-candidate trial (T-083): a check after `gate baseline write`
printed `0 errors, 0 warnings` with no count of the findings the baseline holds back, so a clean
summary looked the same as a clean tree. It waited for T-080, which rewrote `Findings::finish` and
changed which findings reach the baseline (a finding on a target a rule only re-judged for its
exceptions is dropped before the baseline sees it).

## Change

- `Findings::finish_counted` returns the findings `finish` keeps and how many findings an entry of
  the baseline accepted, counted out of the same `Baseline::accept` call that drops them, after
  the exceptions and the re-judged drop. `finish` keeps its signature and returns the findings.
- `Report` carries the count as `baselined`. The summary line adds
  `, N finding(s) held back by gate/baseline.json` after the warnings when N is above 0,
  `--format json` has `"baselined": N` always, and SARIF has it as the run's
  `properties.baselined`.
- The review: identical findings are deduplicated before the baseline sees them, not only after.
  `gate baseline write` stored one entry for two identical findings, and the next check held back
  one and reported the other, so a freshly written baseline failed its own check. This was on
  `main` before; it is fixed here because it sits in the same code.

## Tests

- `finish_counted` counts each finding a baseline entry accepts, not one an exception accepts or a
  core finding, and not an entry with no finding (that entry stays a stale error).
- The summary with and without held findings; the JSON key.
- The CLI: `gate baseline write`, then `gate check` prints the count; a fixed finding lowers it.
- The CLI: a baseline written over two identical findings passes its own check with
  `1 finding held back`.

## Resume point

Code, tests and the review's fix round committed; round 432 queued.

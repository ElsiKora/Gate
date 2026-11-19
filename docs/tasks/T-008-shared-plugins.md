# T-008: Shared plugins git, ci, markdown, agents

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 004: public presentation" |
| Reviewer | a separate review session |
| Depends on | T-003 (the SDK surface is fixed; the runtime may land while this is written) |
| Touches | `plugins/{git,ci,markdown,agents}/**`, their rule pages, the workspace members and `deny.toml` if a dependency needs it; the `git/main-history` and `agents/rule-enforcement` tests in `crates/gate/tests/plugins.rs` |

## Goal

Four shared plugins, each a `wasm32-wasip2` component built with `gate-sdk`, versioned on its own, that
report the same findings as the reference implementation's checks they replace, on the same trees
([architecture.md](../architecture.md) sections 4, 6 and 10).

## Scope

1. One crate per plugin under `plugins/<name>`, a `cdylib` for `wasm32-wasip2`, exports through
   `gate_sdk::plugin!`, version from `CARGO_PKG_VERSION`, its own `CHANGELOG.md`.
2. The rules and generators of each plugin, their options (typed, with a generated schema) and aspects,
   exactly as the private port brief for that plugin lists them (the local rules file names where the
   briefs are). Plugin-level options go in `[plugins.<name>] options`; per-rule options stay on the rule.
3. Generators (the `agents` plugin's files and blocks, any other generator in the briefs) return files
   through `generate`; drift is reported by the core as `gate/sync#<item>`.
4. History rules (`git`) declare `READS_HISTORY` and read only through `run.git()`; they never read the
   clock or the environment (architecture section 16).
5. A page per rule: what it checks, options with an example, an example finding, how to fix it.
6. Tests per rule with `gate-test` / `gate_sdk::check` and a test double host: each pass and each fail,
   each aspect, options errors. Fixtures are synthetic, written for the test.

Not in scope: `ledger` (T-009), `nestjs` (T-007), product plugins, the acceptance harness itself.

## Done when

- fmt, clippy (native and `wasm32-wasip2`), test, deny exit 0; each plugin builds as a release
  component.
- Acceptance outside the repository: the acceptance harness (or the writer, with the same cases)
  shows, for every rule, zero findings on the clean reference trees and identical findings on the
  mutation and replayed cases; the commands and counts are written in the private
  task area.
- Generated files are byte-identical to the reference implementation's on the reference trees.
- The leak scan named in the local rules prints nothing.

## Resume point

2026-11-17T18:45:53+00:00: the verification review's last fix round, on main (T-013 and
T-015). `git/main-history` has no `generated` option: for each file a direct commit changes,
`sync-state` at that commit decides first (drift is reported, a produced file in sync is allowed), a
removed path that `gate sync` produced at the parent is allowed, then `documentation`, and anything
else is a code path. A release commit whose `package-lock.json` does not read as JSON on one side is
reported. Its options refuse a `release-subject` without a capture group and a `branch-length`
minimum above its maximum, and a rule file that is not UTF-8 is a finding, not an error. `agents`
has no `legacy` enforcement class; `instruction-references` checks commands only with a
`command-pattern`; adapters skip files without front matter, strip one `.md` in any case and refuse
two rule files that map to one adapter; the block refuses an `entry` that is not a plain path, links
relative to it and escapes `|`; `rule-references` reads non-UTF-8 markdown lossily. Through the real
host, `agents/rule-enforcement` sees a norm mapped on a `git/*` rule, and `git/main-history` reports
hand-edited and stale generated files and allows those that `gate sync` removes or renames. The full
round exits 0 at every step (458 tests passed), and the acceptance
outside the repository on main's gate has no unexpected difference. In review.

Merged to main with every gate exit 0 and 458 tests (the merge message records
them). Open: `rule-enforcement` compared in full once every declared plugin loads.

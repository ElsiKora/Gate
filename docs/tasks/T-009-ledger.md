# T-009: Plugin ledger

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | T-003 (the SDK surface is fixed), after T-005's review round |
| Touches | `plugins/ledger/**`, their rule pages, the workspace members and `deny.toml` if a dependency needs it |

## Goal

The shared plugin `ledger`, a `wasm32-wasip2` component built with `gate-sdk`, versioned on its own,
that checks a repository's records (plans, decisions, implementation and verification entries) and
reports the same findings as the reference implementation's checks it replaces, on the same trees
([architecture.md](../architecture.md) sections 4, 6 and 10).

## Scope

1. One crate, `plugins/ledger`, a `cdylib` for `wasm32-wasip2`, exports through
   `gate_sdk::plugin!`, version from `CARGO_PKG_VERSION`, its own `CHANGELOG.md`.
2. Its rules, their options (typed, with a generated schema) and aspects, exactly as the private port
   brief lists them (the local rules file names where the
   briefs are). Plugin-level options go in `[plugins.<name>] options`; per-rule options stay on the rule.
3. Records are read as tracked files of the scope plus the configured `reads`; cross-project records
   are reached only through `reads`.
4. Rules that need history declare `READS_HISTORY` and read only through `run.git()`; they never read the
   clock or the environment (architecture section 16).
5. A page per rule: what it checks, options with an example, an example finding, how to fix it.
6. Tests per rule with `gate-test` / `gate_sdk::check` and a test double host: each pass and each fail,
   each aspect, options errors. Fixtures are synthetic, written for the test.

Not in scope: the other shared plugins (T-008), `nestjs` (T-007), product plugins, the acceptance harness itself.

## Done when

- fmt, clippy (native and `wasm32-wasip2`), test, deny exit 0; the plugin builds as a release component.
- Acceptance outside the repository: the acceptance harness (or the writer, with the same cases)
  shows, for every rule, zero findings on the clean reference trees and identical findings on the
  mutation and replayed cases; the commands and counts are written in the private
  task area.
- The leak scan named in the local rules prints nothing.

## Resume point

The crate, its nine rules, their pages and tests are written; the acceptance ran (clean trees, mutation
cases, replayed cases) and is recorded in the private task area. The review round is in: option paths
must be in normal form, every option is checked before a rule looks at the tree, `closed-statuses` must
be lowercase, one parser reads the decision index for `decisions` and the other rules, `closing-notes`
is off until `field` is set, and the aspect `decisions#record-status` is `decisions#decision-status`.
`exception-owners` accepts only an open owner, planned or in progress. The branch has main merged in
and builds on the workspace `crates/gate-sdk`. Next: the lead's merge.

Open: `exception-owners` is accepted as partial. Its six failing mutation cases put the owner on an
exception of another plugin. Without a component of that plugin the exception is dropped, so the
cases run but their difference is explained by the dropped exception instead of compared. They are
compared once a component of that plugin built from main exists (after T-007a and T-007b), given beside
the ledger build, comparing only the ledger rules.

Merged to main with every gate exit 0 and 373 tests (the merge message records
them). Open: the six exception-owners cases, compared again with a nestjs component built on main.

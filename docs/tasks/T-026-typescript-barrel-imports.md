# T-026: `typescript`: a name its folder's barrel does not export is imported only from inside the folder

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | none (from `release/v0.1.0` or main after v0.1.0) |
| Touches | `plugins/typescript/**` (one new rule, its options schema, its page), its tests and fixtures |

## Goal

The second consumer's structure requirement (plan, "From the second consumer's requirements"):

- A module that imports from a folder with a barrel (`index.ts` by default, an option) from outside that
  folder imports only names the barrel exports, and imports them through the barrel, not through a deep
  path into the folder.
- Kind children stay importable within their owner: a folder's own subfolders may import each other
  and the owner's files directly (the owner is configured by glob).
- Test roots are outside the folder unless the configuration names them exempt (globs).

## Scope

1. One rule, `typescript/barrel-imports`, with aspects `deep-path` (a path past the barrel) and
   `not-exported` (a name the barrel does not export). Options validated by its schema: `barrel-name`,
   `owners` (globs), `exempt` (globs). It reads only through the plugin interface, using the parse the
   plugin already has; type-only imports follow the same rule.
2. Its rule page in the plugin's docs, in the house style of the other rule pages.
3. Synthetic fixtures only (no file from any consumer): pass and fail for each aspect, a kind child
   importing within its owner (pass), a test root outside (fail) and exempt (pass), re-exports through
   `export * from` and `export { a as b }`, and a barrel that does not exist (no finding).

## Done when

- The tests above, each failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- The lead runs it on the second consumer's tree and sends that consumer the paths it reports.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: rule, schema, page and synthetic tests on `task/T-026-typescript-barrel-imports`,
review round 1 applied (`export *` cycles resolve as a closure, tsconfig aliases and
`import x = require` tested). Rebased onto the public `main` 59fb163; the workspace round on the
rebased tip runs in the lane next, then the merge.

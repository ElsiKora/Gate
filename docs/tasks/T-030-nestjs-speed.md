# T-030: `nestjs` runs within the check's time bound on a large tree

| Field | Value |
| --- | --- |
| Status | planned |
| Release | v0.2.0 |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | the v0.1.0 root (rebase onto it once it exists) |
| Touches | `plugins/nestjs/**` and the TypeScript library it uses; their tests |

## Goal

On the second consumer's converted tree (about 10 450 tracked files) the plugin takes 38.3 s cold for
37 rules with the rc.1 core (`gate check --rule 'nestjs/*' --timings`), and it runs out of a fuel of
10^11. That consumer's bound for the whole non-history check is 3 s warn and 6 s fail, cold and warm.
The plugin's share must fit in it, with T-025 making the warm run cheap.

## Scope

1. Measure per rule, in time and in fuel, and say which rules and which shared work (parsing, module
   resolution, the symbol index) dominate.
2. Parse and index each file once per call, shared by every rule of the call, rather than once per
   rule; resolve each import once; read only the files a rule's globs select.
3. No finding changes: the plugin's tests and the acceptance cases stay identical.
4. Tests: the work counted, not timed (files parsed per call equals files in scope, whatever the rule
   count).

## Done when

- The tests above, each failing before the change.
- Measured on a synthetic tree of that size in the repository's tests, and by the lead on the
  consumer's copy: cold time and fuel per rule, before and after.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: planned; branch `task/T-030-nestjs-speed`.

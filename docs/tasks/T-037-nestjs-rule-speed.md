# T-037: the `nestjs` rules' own work fits one call in 3 s

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 if green and reviewed before the release round starts; otherwise 0.2.1 |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | T-030 (one parse, one read and one resolution per file per call), merged into `release/v0.2.0` |
| Touches | `plugins/nestjs/**`, the TypeScript library it uses; their tests |

## Goal

T-030 shares the parse, the read and the import resolution of each file between the rules of one
call. On the synthetic service of `plugins/nestjs/tests/large` (10 668 files, the 30 rules that run
with default options) one call of every rule then takes 10.0 s in a release build, against 20.5 s
before (lane log `both-run1`, `cargo test --release --locked -p gate-plugin-nestjs --test speed --
--ignored --nocapture`). The rest is each rule's own work. What each rule adds to a call that has
already parsed the tree, from the same run: `barrel-exports` 4.2 s, `automator-metadata` 2.5 s,
`field-inventory` 1.8 s, `controller-owners` 1.3 s, `native-repository` 1.2 s, `typeorm-relation`
1.2 s, every other rule under 1 s (each figure about ±0.4 s).

The target: one call of every rule within 3 s on that synthetic tree, release build.

## Scope

1. For each of the six rules, find the work that grows faster than the tree (a scan per file of
   every other file, an index rebuilt per file, an export closure walked again per import) and
   compute it once per call or once per file. Indexes several rules need (the owner tables, the
   binder, the module loads) go through the call's shared state, as T-030's parses do.
2. Barrel-exports first, then the other five in the order above.
3. No finding changes: the plugin's tests and the acceptance cases stay identical; the synthetic
   tree's findings stay at the same count (32 486).
4. Tests count work, not time, where the work is countable (for example, the export closure of a
   barrel computed once per call), each failing before its change.

## Done when

- One call of every rule within 3 s on the synthetic tree, release build. Every measurement names
  the command and the lane ticket.
- The tests above, each failing before its change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: task file written on `task/T-037-nestjs-rule-speed`, from `release/v0.2.0` 264264f. The code work
starts once T-030 is in `release/v0.2.0`; this branch is then rebased onto it.

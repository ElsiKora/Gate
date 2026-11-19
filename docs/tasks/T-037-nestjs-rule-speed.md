# T-037: the `nestjs` rules' own work fits the check's bound on a large tree

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | 0.2.1, from the `v0.2.0` tag |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | T-030 (one parse, one read and one resolution per file per call), in `v0.2.0` |
| Touches | `plugins/nestjs/**`, the TypeScript library it uses; their tests |

## Goal

T-030 shares the read, the parse and the import resolution of each file between the rules of one
call. On the synthetic service of `plugins/nestjs/tests/large` (10 670 files, the 30 rules that run
with default options), native release build, one call of every rule went from 177 694 reads and
166 827 parses to 10 674 and 10 668, and its CPU time from 1.65 s to 0.80 s; its wall time after
T-030 was 1.18 s and 1.20 s in the two quiet tickets (`cargo test --release --locked -p gate-plugin-nestjs --test speed --
--ignored --exact measure --nocapture`, lane tickets 191, 194 and 200). The rest is each rule's own
work. What each rule adds to a call that has already parsed the tree (`measure`, ticket 191):
`field-inventory` 0.70 s, `barrel-exports` 0.41 s, `automator-metadata` 0.37 s,
`native-repository` 0.24 s, `wrappers` 0.18 s, `native-functions` 0.18 s; the figures move by a
few tenths between tickets on the shared machine.

The synthetic tree in native code is already under 3 s, so it is not the target. The bound that
matters is the consumer's: 3 s warn and 6 s fail for the whole non-history check, cold and warm,
with the plugin running in wasm under the host's fuel. The target: `gate check --rule 'nestjs/*'
--timings` on the lead's copy of the consumer tree (about 10 450 files) fits that bound cold, and
no rule's fuel grows faster than the tree. The lead measures it; the synthetic `measure` stays the
regression check in this repository.

## Scope

1. Measure first: the per-rule time and fuel of the consumer run (from the lead) next to the
   synthetic `added` figures, and pick the rules from both.
2. For each chosen rule, find the work that grows faster than the tree (a scan per file of
   every other file, an index rebuilt per file, an export closure walked again per import) and
   compute it once per call or once per file. Indexes several rules need (the owner tables, the
   binder, the module loads) go through the call's shared state, as T-030's parses do.
3. Barrel-exports first (the lead's order), then the others as the measurement ranks them.
4. No finding changes: the plugin's tests and the acceptance cases stay identical; the synthetic
   tree's findings stay at the same count (32 488).
5. Tests count work, not time, where the work is countable (for example, the export closure of a
   barrel computed once per call), each failing before its change.

## Done when

- `gate check --rule 'nestjs/*' --timings` on the consumer copy within the bound, cold, measured
  by the lead; the synthetic `measure` before and after each cut. Every measurement names the
  command and the lane ticket.
- The tests above, each failing before its change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: T-030 is merged into `release/v0.2.0` (1b7fb64). The goal is restated on its numbers:
the synthetic tree is already within 3 s natively, so the target is the consumer run in wasm. Next:
once `v0.2.0` is tagged, rebase this branch onto the tag; queue nothing before the release round
ends; ask the lead for the consumer run's per-rule time and fuel; then cut barrel-exports first.

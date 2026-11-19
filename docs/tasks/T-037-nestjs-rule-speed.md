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

2026-11-19: the first cut is in: barrel-exports, and with it every per-folder walk of the plugin.
The shared walks (`tree::files`, `entries`, `folders`) scanned every tracked path for each folder
they walk below; they now take the source's range of paths below the folder
(`Source::paths_with_prefix`, answered by a binary search in the sorted sources). The review of
d688aee asked for the test fixes of 54acbcf and the proof below. Next: the second cut, chosen from
Gate 005's per-rule time and fuel on the consumer tree (lane ticket 205). The synthetic `added`
figures no longer rank the rules: with one call under a second, a rule's added time moves by more
than the rule takes alone (repeated-segment: 0.006 s alone, 1.032 s added in ticket 203).

## Proof

Lane ticket 203, `round1.sh`, one ticket for all of it:

- d688aee: `cargo fmt --all --check` 0; `cargo clippy --locked --workspace --all-targets -- -D
  warnings` 0; `cargo test --locked --workspace --no-fail-fast` 0 (1015 passed, 0 failed, 7
  ignored); `cargo build --release --locked` 0. `cargo deny --locked check` exited 1 one second in,
  output not kept; a deny-only lane job keeps it (see below).
- 2e5bf04, the test before the cut: `cargo test --locked -p gate-plugin-nestjs --lib
  shared::tree::tests` exit 101, as it should (`800 paths read` for a walk below one folder of
  four files).
- The synthetic `measure` (`cargo test --release --locked -p gate-plugin-nestjs --test speed --
  --ignored --exact measure --nocapture`, exit 0 both), 10 670 files, 30 rules, native release
  build, each tree built into its own target directory:

| | Before (main 89c42c7) | After (d688aee) |
| --- | --- | --- |
| One call of every rule | 2.164 s | 0.887 s |
| Sum of the single-rule calls | 3.172 s | 1.593 s |
| barrel-exports alone | 0.485 s | 0.188 s |
| What barrel-exports adds to a parsed call | 0.611 s | 0.192 s |
| Reads, parses, findings of one call | 10 674, 10 668, 32 488 | 10 674, 10 668, 32 488 |

Wall times on the shared machine move with its load; the two columns ran minutes apart in one
ticket.

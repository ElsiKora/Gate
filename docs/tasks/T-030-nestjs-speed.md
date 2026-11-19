# T-030: `nestjs` runs within the check's time bound on a large tree

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | the v0.1.0 root (rebase onto it once it exists) |
| Touches | `plugins/nestjs/**`, the TypeScript library it uses, and `crates/gate-sdk` for the plugin's call state (`CallState`, `Call`, `Run::with_call`, the state parameter of `Rule`, `Run`, `RuleEntry` and `PluginSpec`); their tests |

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

2026-11-19, review fix round, before the rebase. The design after the review: no `self_cell` and
no unsafe, transmute or leak anywhere. The SDK has a plugin-defined state per `check` call,
`CallState` (an owner built from the `Call`, and a state that may borrow it), declared with `call:`
in `plugin!` and reached by a rule through `Run::with_call`; the state parameter defaults to `()`,
so the other six plugins are unchanged. It replaces the earlier `Run::call_cache`, which is gone;
nothing else needed it. The nestjs state (`Nest`) owns the allocator, the call's read cache and the
settings, and lends one TypeScript view: one parse per file and one resolution per import for the
whole call. A file's bytes leave the read cache once the view parses it (a non-UTF-8 file keeps
them); the semantic model is built lazily, once per parsed file, as on 59fb163. Tests: the counting
test asserts one read per file, the parse count of the whole call against the files in scope, and
the findings of one call equal to the per-rule findings joined in rule order, on a tree with a file
that does not parse and one that is not UTF-8; the library tests resolve through two views with
different tsconfigs in one call and count a view's parse. The round passed (lane ticket 191) on 99dffd7; the numbers are below. Next: the rebase onto
`origin/release/v0.2.0` and its round.

## Numbers

Synthetic service of 10 670 files (10 668 TypeScript sources), 30 default rules, release build.
Before is 59fb163 with only the parse counter and the new tests added, built into its own target
directory; after is 99dffd7. Commands: `cargo test --release --locked -p gate-plugin-nestjs --test
speed -- --ignored --exact <test> --nocapture` for `measure` and `parse_alone`, and the same test
binary under `/usr/bin/time -l` for `tree_alone` and `one_call`. Lane tickets 191 (after) and 194
(before twice, after again). The machine is shared: wall times move with its load (the same after
build took 1.180 s in ticket 191 and 8.463 s in ticket 194), so the CPU time of the one-call
process is the steadier figure.

| One call of every rule | Before (194, twice) | After (191; 194) |
| --- | --- | --- |
| Wall time, `measure` | 15.456 s; 14.781 s | 1.180 s; 8.463 s |
| CPU time (user + sys), `one_call` | 1.65 s; 1.69 s | 0.78 s; 0.82 s |
| Reads from the host | 177 694 (10 673 files) | 10 674 (10 674 files) |
| Parses | 166 827 | 10 668 |
| Findings | 32 488 | 32 488 |
| Peak memory footprint, `one_call` | 204.1 MB; 203.8 MB | 294.7 MB; 293.6 MB |
| Maximum resident set, `one_call` | 298.9 MB; 303.3 MB | 308.7 MB; 308.4 MB |
| The tree alone (`tree_alone`), footprint | 10.5 MB | 10.4 MB |
| Parse of every source, no semantic model (`parse_alone`) | 0.071 s; 0.082 s | 0.046 s; 0.094 s |

Memory is the native process's (the test runs the plugin natively, not in wasm), so it is not the
plugin's linear memory under the host. The call keeps every parsed file until it ends, where
before each rule dropped its own: about 90 MB more at peak. The counting test fails before (exit
101: 288 of 291 files read more than once) and passes after. `measure` runs natively and reports no
fuel; fuel per rule comes from `gate check --timings` on the lead's copy of the consumer tree.

What each of the next cuts adds to a call that has already parsed the tree (`measure`, the call of
`string-enums` with the rule less `string-enums` alone), after, tickets 191 and 194:
barrel-exports 0.41 s and 4.24 s, field-inventory 0.70 s and 1.21 s, automator-metadata 0.37 s and
1.14 s, native-repository 0.24 s and 0.74 s, controller-owners 0.04 s and 0.84 s; these go to T-037.

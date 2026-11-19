# T-001: Measure the plugin form

| Field | Value |
| --- | --- |
| Status | done |
| Release | R1 (decision input) |
| Writer | "Gate 002: plugin form spike" |
| Reviewer | the lead reads the numbers |
| Depends on | none |
| Touches | branch `task/T-001-plugin-form-spike` only (`spike/` at the repository root); never merged |

## Goal

Numbers that settle how a plugin runs: a WebAssembly component run by the core (wasmtime, embedded,
read-only access to the tree through host calls), or a native process per plugin over a small
protocol. The heaviest rules are TypeScript rules on the largest consumer tree; a form several times
slower than today's in-process run on those rules is a regression.

## Scope

1. Pick the workload from the reference implementation (its TypeScript layer and its NestJS rules,
   outside this repository): parsing every tracked `.ts` file with oxc plus two or
   three of the heaviest rules that walk whole syntax trees and resolve modules (candidates: the field
   inventory, the import/dependency rules, the owner graph built by `Sources`). Ask the lead for the
   baseline per-rule timings if they have arrived (task T-000); otherwise pick by reading the code.
2. Build that workload three ways from the same Rust source, in `spike/`:
   - **native**: in-process, as the reference implementation runs;
   - **wasm**: a `wasm32-wasip2` component (add the target with `rustup target add wasm32-wasip2` for the
     pinned toolchain), run by a small host binary embedding `wasmtime` (latest release on crates.io),
     with the tree given to the guest either as a read-only preopened directory or through host calls
     `list-files` / `read-file` (measure both if cheap); compile once, report compile time and the cached
     (`Module::serialize` / `deserialize`) start;
   - **process**: the native workload as a separate executable driven over stdin/stdout (length-prefixed
     JSON or similar), to measure protocol cost.
3. Measure on the read-only export of the largest consumer tree (the local rules name it; about 9900
   `.ts` files): wall time, user time, max RSS, 5 runs each through the queue, median and spread. Also
   time: parse-only in each form; one plugin vs three plugins in parallel each parsing the whole tree
   (the cost of each plugin parsing on its own); the cost of a host call per file read.
4. Measure one cross-boundary shape: passing a parsed tree from one component to another (serialize the
   oxc AST or a flat node table) vs each plugin parsing itself, on the same tree.

Not in scope: a plugin API design, product code, anything merged to `main`.

## Done when

- `spike/RESULTS.md` on the task branch: a table of every number with the command that produced it, the
  machine load (`uptime`) at each run, and a short conclusion: wasm / native ratio on parse and on each
  rule, the parallel three-plugin wall time, and which form you would choose and why.
- Branch pushed; a message to "Gate Lead" with the table.

## Notes

- Local rules: the local rules file named in the brief (consumer trees, the queue, scratch folders).
- Scratch: the `T-001/` folder of the local scratch directory. Export the tree there with `git archive`.
- Build wasmtime once; it is large. Use `--release`. Put a hard timeout on every run.

## Resume point

2026-11-16T05:05:07+00:00: done. `spike/RESULTS.md` holds the table (every number with its command),
the per-phase comparison, host-call and WASI read costs, three plugins at once, compile and load
times, and the hook case against today's `check --staged --no-history`. The form chosen from it is
WebAssembly components (architecture section 3). Not run, by the lead's call once the form was
decided: the node-table rows (`table-*`) and the cold-compile and process-parse rows. Correction to
the first report: instructions retired are not load-stable on this machine; compare forms
interleaved or by phase minimums.

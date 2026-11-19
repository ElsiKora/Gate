# T-001 results: the plugin form

Status: done, 2026-11-16. The form chosen from these rows is WebAssembly components
([architecture.md](../docs/architecture.md) section 3).

## What was measured

One Rust workload (`spike/workload`), built three ways from the same source:

| Form | How it runs | Tree access |
| --- | --- | --- |
| native | in the host process, on a thread with an 8 MiB stack (as the reference implementation runs: one thread, one allocator per run) | `std::fs` |
| wasm | a `wasm32-wasip2` component (`spike/guest`, 8 MiB guest stack), run by `spike-host` embedding wasmtime 50.0.1 (default `Config`, Cranelift), precompiled with `Component::serialize` and loaded with `deserialize_file` | `host`: the `tree.list-files` / `tree.read-file` host calls of `spike/wit/plugin.wit`; `fs`: a read-only WASI preopen of the tree at `/tree` (the listing still comes from one host call) |
| process | `spike-plugin`, a separate executable driven over stdin/stdout with frames of `u32` length, kind byte, payload (JSON messages, raw bytes for file contents and tables) | `fs`: the plugin reads the directory itself; `rpc`: one request/response round trip per file read |

In every form the host lists the tracked files (as git does) and hands the listing over once.

The workload, on the export of a TypeScript repository (11 396 tracked files, 9 881 `.ts`, 34.2 MB of
TypeScript; `find tree -name '*.ts' | wc -l`, `... | xargs cat | wc -c`):

| Task | Shape of | What it does |
| --- | --- | --- |
| `read` | the file access of every rule | reads every `.ts` file, no parse |
| `parse` | the parse rule (and the first parse every TypeScript rule triggers) | oxc 0.153 parse of every `.ts` file into one allocator that lives for the run |
| `walk` | module-structure rules, the field and declaration inventories | a full `oxc_ast_visit::Visit` over every tree, counting nodes, classes and decorators on classes and properties |
| `semantic` | rules that read bindings | `SemanticBuilder` scopes, symbols and references for every source file below `src/` |
| `resolve` | the import and dependency rules, the owner graph | every import and re-export specifier resolved with `oxc_resolver` 11.24.3 over the tracked file set with the root tsconfig `paths` (the reference implementation's resolver configuration), then the import cycles (iterative Tarjan) |
| `table` | passing a parsed tree between plugins | plugin A parses and writes a flat node table (path, source, one `(type, start, end, parent)` record of 16 bytes per node); the host carries the bytes to plugin B, which runs the decorator count of `walk` on the table; compared with B parsing itself |

Every outcome carries counts (files, bytes, nodes, decorators, symbols, specifiers, cycles); the forms
are compared only where the counts are identical.

## Commands

Builds went through the machine's job queue; the measurement runs ran directly, one form at a time
(three processes for the `-x3` rows), under `nice -n 5`, each run under `/usr/bin/time -l` inside
`timeout`, with `uptime` sampled before it. The machine has 18 cores and was shared with other
projects' jobs: the 1-minute load during the runs was 48 to 137.

```bash
cargo build --release --locked -p spike-host -p spike-plugin
cargo build --release --locked -p spike-guest --target wasm32-wasip2
# batch 1 (04:48Z) and batch 3 (04:54Z): every configuration 5 times in a row; HOOK and GATE add the hook rows
HOOK=<hook-tree> GATE=<the reference implementation's binary> RUNS=5 nice -n 5 spike/bench/run.sh <spike-host> <spike-plugin> <spike_guest.wasm> <tree> <out-dir> [<config-regex>]
# interleaved (05:01Z): 3 rounds, each configuration once per round
INTERLEAVE=1 RUNS=3 nice -n 5 spike/bench/run.sh ... <out-dir-2> '^(native|wasm-host)-(parse|walk|semantic|resolve|all)$'
# every table below
spike/bench/table.py <out-dir> <out-dir-2>
```

Every run exited 0 and every form reported the same counts (`same_counts` and the `outcomes` of each
run): 9 881 files parsed, 3 376 945 nodes, 1 722 classes with 2 532 class and 7 932 property
decorators, 7 405 semantic files with 61 643 symbols and 8 300 unresolved names, 48 966 specifiers of
which 39 233 resolve, 22 import cycles over 142 files, no fatal parse.

## Numbers

Median (min-max) per configuration. G instructions and max RSS are of the measured process; CPU is
user + system of it and its children.

| configuration | runs | load | G instructions | CPU s | wall s | max RSS MB |
| --- | --- | --- | --- | --- | --- | --- |
| native-read | 5 | 91 (89-94) | 4.80 (4.69-5.44) | 0.82 (0.68-1.06) | 2.44 (1.68-3.82) | 9 |
| wasm-host-read | 5 | 87 (85-88) | 4.85 (4.68-5.41) | 0.83 (0.71-1.16) | 2.60 (1.84-6.67) | 17 |
| wasm-fs-read | 5 | 84 (77-88) | 18.43 (17.81-18.98) | 4.50 (4.02-4.81) | 22.35 (15.99-33.27) | 19 |
| process-rpc-read | 5 | 64 (60-72) | 5.72 (5.35-5.83) | 1.59 (1.38-1.76) | 7.08 (4.85-8.90) | 10 |
| native-parse | 8 | 124 (75-137) | 7.06 (6.17-7.34) | 1.73 (1.04-1.91) | 10.18 (1.26-13.51) | 226 |
| wasm-host-parse | 8 | 127 (71-133) | 8.02 (7.22-8.27) | 1.71 (1.18-2.10) | 6.70 (2.33-10.00) | 175 |
| native-walk | 8 | 74 (66-99) | 6.34 (6.17-7.07) | 1.15 (0.92-1.45) | 1.91 (1.34-3.75) | 226 |
| wasm-host-walk | 8 | 58 (55-92) | 7.47 (7.41-7.82) | 1.33 (1.23-1.57) | 3.20 (1.66-5.71) | 175 |
| native-semantic | 8 | 68 (64-94) | 6.56 (6.32-6.90) | 1.16 (0.98-1.36) | 1.98 (1.46-3.65) | 227 |
| wasm-host-semantic | 8 | 57 (55-85) | 7.84 (7.63-8.61) | 1.50 (1.26-2.08) | 4.44 (1.63-7.12) | 176 |
| native-resolve | 8 | 62 (59-92) | 7.25 (7.15-7.92) | 1.27 (1.16-1.74) | 2.37 (1.53-5.10) | 239 |
| wasm-host-resolve | 8 | 58 (53-85) | 9.39 (8.57-9.72) | 2.00 (1.46-2.34) | 8.25 (1.66-11.83) | 185 |
| native-all | 8 | 116 (68-126) | 7.71 (7.34-8.17) | 1.42 (1.17-1.76) | 4.14 (1.44-8.54) | 241 |
| wasm-host-all | 8 | 102 (73-112) | 9.08 (8.96-9.89) | 1.52 (1.36-1.97) | 4.38 (1.59-8.24) | 185 |
| native-all-x3 | 5 | 57 (57-58) | 16.98 (16.78-17.17) | 3.29 (3.19-3.63) | 3.88 (2.91-4.22) | 705 |
| wasm-host-all-x3 | 5 | 68 (55-71) | 22.59 (21.56-23.74) | 4.46 (3.69-4.82) | 4.71 (1.61-7.91) | 528 |
| process-fs-all-x3 | 5 | 66 (63-66) | host only | 4.14 (3.22-4.56) | 3.35 (1.55-7.72) | 239 per process |
| compile | 5 | 112 (112-117) | 10.10 (10.07-10.14) | 1.18 (1.11-1.21) | 0.53 (0.41-2.01) | 110 |

`walk`, `semantic` and `resolve` rows are `parse` plus that one task; `all` is parse plus the three.
The `-x3` rows run three plugins at once, each listing, reading and parsing on its own.

### What a rule costs inside the plugin

Instructions retired grow with machine load (the kernel side of file reads) and vary by about 0.5 G
between runs, which is more than one rule costs, so a rule's cost is not the difference between two
rows. The workload clocks its own phases; under load the fastest run of a configuration is the least
disturbed, so these are the minimum phase times in ms over all runs of each row (`table.py`):

| phase | native | wasm (host calls) | wasm / native |
| --- | --- | --- | --- |
| read 9 881 files (`parse.read`) | 630 | 693 | 1.10 |
| oxc parse (`parse.oxc`, the parse rule's shape) | 200 | 329 | 1.65 |
| full AST walk (`walk`, module-structure and inventory shape) | 28 | 26 | 0.94 |
| scopes, symbols, references (`semantic`, the binding rules' shape) | 32 | 37 | 1.16 |
| resolve every specifier and the cycles (`resolve`) | 107 | 132 | 1.24 |

From the `-all` rows; the single-task rows give the same picture (parse 1.40 to 1.96, semantic 1.31,
resolve 1.25, walk 0.89). In total the least-disturbed parse and the three rules take about 1.0 s
native and 1.2 s as a component.
`semantic` builds scopes and symbols without the AST node table (`with_build_nodes`), which some
binding rules also build; that part is not measured.

### Host calls and the file system

| access | G instructions over native-read | per file read |
| --- | --- | --- |
| wasm, `tree.read-file` host call | +0.05 | within noise |
| wasm, WASI preopen of the tree | +13.6 | about 1.4 M instructions |
| process, one pipe round trip per read | +0.9 (host side only) | about 93 k instructions |

A host call that copies the file's bytes into guest memory costs what a native read costs. Reading
through the WASI file system costs 3.8x the instructions of the native read. The runtime therefore
gives plugins no preopened directory: the tree is reached only through `tree.read`.

### Three plugins at once

Three plugins, each reading and parsing the whole tree, take 3.9 s wall native, 4.7 s as components
in one host and 3.4 s as three processes, against 4.1 and 4.4 s for one plugin. The components share
one engine and one compiled module and peak at 528 MB for all three, against 705 MB native (three
226 MB allocators) and three processes of 239 MB. No tree is passed between plugins; each parses what
it reads. The node-table rows (`table-*`, plugin A parses and hands a flat node table to plugin B) were
built but not run: the form was decided before them.

### Compiling and loading

The 1.75 MB component compiles with Cranelift in 383 ms (352-1 837) into a 3.58 MB artifact; loading
that artifact (`Component::deserialize_file`) takes 0.9 ms (median of the run rows) and instantiation
1.3 ms. The compiled-module cache is therefore the whole startup cost: once per plugin version.

### The hook case

A copy of the tree as a git repository with one `.ts` file below `src/` changed (one exported constant
appended) and staged:

| configuration | runs | load | CPU s | wall s | max RSS MB |
| --- | --- | --- | --- | --- | --- |
| the reference implementation's `check --staged --no-history` | 5 | 65 (61-67) | 4.54 (4.44-4.82) | 15.11 (7.12-19.23) | 744 |
| wasm component, all tasks, compiled module cached | 5 | 51 (48-54) | 2.12 (1.52-2.27) | 6.55 (2.58-8.60) | 185 |
| native, all tasks | 5 | 57 (54-62) | 1.76 (1.18-2.43) | 3.47 (1.43-13.21) | 241 |

The reference implementation's `--staged` selects its checks but each runs over the whole tree (its
`--timings` put the parse check at 6.48 s in the first run), so the hook costs as much as the full run. The component row re-runs every
task over every file too: it is the upper bound with no result cache. With the read-set result cache
of architecture section 9, a one-file change re-runs only the calls whose read set holds that file.
That cache is not built yet, so its effect is not measured here. The rows ran one after another, not
interleaved. The instructions of the reference implementation's row are those of the `sh` that starts it, so they are left out.

## Conclusion

1. WebAssembly components (WASI 0.2, wasmtime, tree through host calls) cost 1.1x native on reading
   files, 1.65x on the oxc parse and 0.9x to 1.25x on the rule shapes, and use a quarter less memory.
   The whole parse plus three heavy rules is about 1.2 s.
2. Never give plugins a WASI file system: reading through it costs 3.8x. Cache the compiled module: compiling
   takes 0.4 s, loading the artifact 1 ms.
3. The hook meets the speed bound only with the read-set result cache. Without it a one-file change
   costs a full run in either form. Instructions retired are not load-stable on a shared machine.
   Compare forms only interleaved, or by the phase minimums above.

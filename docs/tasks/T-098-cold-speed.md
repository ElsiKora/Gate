# T-098: a cold check without history under 6 s

| Field | Value |
| --- | --- |
| Status | approved (`3736fa66`); steps 2 to 7 being written, then measured |
| Release | the next plugin release (the lead cuts it); no version moves here |
| Writer | "Gate 098" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `df718c0` (`v0.3.1` released) |
| Touches | `crates/gate-host/src/snapshot.rs`, `crates/gate-host/src/plugins.rs`, `languages/typescript/src/call.rs`, `plugins/nestjs`, `plugins/ledger`, the workspace `Cargo.toml` (one dependency's features), their tests, `CHANGELOG.md` |

## Goal

The first consumer's move waits for a cold `gate check` without git's history rules that counts
under its hook bound, `--warn-after 3 --fail-after 6`, on its tree (compiling is exempt). Cold here
means an empty check cache with the pinned plugin files already fetched and verified. On released
0.3.0 that run counts 6973 ms and fails the bound. The cold pre-push, mostly `git/main-history`, is
held only to "no slower than the reference tool", so git stays as it is.

The aim is a margin, not a pass: **at most 4000 ms counted** on the measuring machine at a 1-minute
load under 18, so that a run on a machine 1.5 times slower (a shared CI runner) still passes 6 s.

## Where the time goes today

Measured on the consumer's converted tree (outside the repository; about 11 thousand tracked files,
70 MB), released core and plugins 0.3.0, 18 cores. Commands: `gate check --timings` with the cold
cache method of the move's measuring script; fuel is deterministic, so it is compared across loads.

- The plugin calls run in parallel, one call per plugin, so the run is as long as its longest call.
  Counted 6973 ms = snapshot 197 + configuration 10 + load 88 (plus 1859 compiling, exempt) +
  plugins 6654. The calls: nestjs 6572 ms (fuel 57.0e9; host `tree.read` 1111 ms in 15267 reads),
  ledger 3381 ms (38.8e9; 574 ms in 13020 reads), typescript 2746 ms (9.8e9; 1345 ms in 21438
  reads), the consumer's own product plugin 2137 ms (11.0e9; 922 ms in 8126 reads).
- **Reading files is a sixth to a half of each call.** A read is a host call that opens the file;
  on this machine an `open` costs about 60 µs, and every directory on the way is `lstat`ed for the
  symbolic-link check. A native run of the nestjs rules with a sampling profiler (a private
  harness, never committed) spends 26 % of its samples in `open` and `read`.
- **Files are read more than once.** The TypeScript call state drops a file's bytes once it is
  parsed; a rule that then asks the call source for the bytes reads the file from the host again
  (nestjs reads about 10.4 thousand distinct files in 15267 reads). Each ledger rule builds its own
  catalog, so the ledger's 1.4 thousand files are read once per rule: 13020 reads.
- **nestjs, fuel per rule.** Fuel of all rules minus the fuel without one rule, 37 runs (released
  core, results emptied each run): `advisory-lock-boundary` 15.9e9, `asset-storage` 8.8e9,
  `module-registry` 3.7e9, `raw-config-access` 3.0e9, `notification-egress` 2.3e9,
  `field-inventory` 2.3e9, `barrel-exports` 1.9e9, `automator-metadata` 1.6e9,
  `advisory-lock-manifest` 1.4e9; every other rule under 1e9. Parsing every source file costs about
  1.5e9: the rules' own work is the cost, not the parse.
- **nestjs regular expressions run on the slowest engine.** The workspace builds `regex-automata`
  with the PikeVM alone (no lazy DFA, no bounded backtracker, no one-pass DFA, no literal
  prefilters). In the native profile a third of `asset-storage` is PikeVM search.
- Inside `advisory-lock-boundary` (native profile): statement extraction 29 % and the
  registered-identifier scan 29 %, the latter testing each registered identifier against a list of
  every number in the file; reading and parsing its files are the rest.
- Each of six rule sites builds its own `owners::Sources` (the per-file import bindings of the
  source root): 8 % of the native samples, most of it repeated work.
- **ledger.** Five rules each call `Ledger::read`, which runs the record grammar over every
  collection again: `records::fields` alone is 38 % of the ledger's native samples.
- Build options were tried and dropped: `+simd128` takes 1.1 % of the nestjs fuel, fat LTO with one
  codegen unit on top 2.1 %, for much longer release builds.

## The change

Every step only removes repeated work or picks a faster engine for the same computation; none
changes what a rule reads (the read set), what it finds, or in what order.

1. **Core: read ahead, in parallel (`gate-host`).** When at least one call misses the result
   cache, the files those calls' grants allow are read into the snapshot's content cache by a
   small pool of threads while the plugins start, in path order. A working-tree snapshot reads them
   as `Snapshot::read` does (the same function, the same symbolic-link check); an index or commit
   snapshot streams the blobs through its own `git cat-file --batch`. A file over 1 MiB is left to
   the plugin's own read, and so is any file once 256 MiB were read ahead. A read-ahead failure is
   dropped: the plugin's read reports it as today. Read-ahead never records a path as asked (the
   configuration's inputs), and the cache keeps the first bytes read for a path, so every plugin of
   a run sees one content per path. A run whose calls all come from the result cache reads nothing
   ahead, so a warm run stays as it is.
2. **Core: the symbolic-link check once per directory.** The check that no directory on the way
   is a symbolic link (`behind_symlink`) is remembered per directory for the snapshot's life.
3. **TypeScript call state: a file's bytes stay with the call (`gate-typescript`).** `CallSource`
   keeps the bytes of a parsed file, so the call source answers a rule from memory instead of a
   second host read. The memory is the size of the parsed sources, once.
4. **TypeScript call state: one `owners::Sources` per source root.** The call state builds the
   bindings of a root once and the rules of the call share them, as they share the parse.
5. **Regex engines.** `regex-automata` gets `hybrid`, `nfa-backtrack`, `dfa-onepass` and `perf`.
   The meta regex reports the same matches whichever engine runs; only the time changes. This also
   speeds `frontend`, the other user.
6. **nestjs hot spots**, each pure: the registered-identifier scan tests membership in a set of
   the file's numbers (integers compare exactly as doubles, as now); `asset-storage` computes a
   file's tokens once per run; then the next rules of the fuel list, profiled again after steps 1
   to 5, until the target is met.
7. **ledger: a call state.** One catalog per call (each ledger file read and fenced-blanked once)
   and `Ledger::read` computed once per call, since it depends only on the tree and the
   `[plugins.ledger]` settings, which are the same for every rule of a call.

Not changed: fuel metering, the deadline and the other limits; the plugin interface
(`gate:plugin@0.3.0`); the result cache key; git and its history rules; CI generation (T-099).
Splitting one plugin call over several instances was considered and left out: it adds CPU on a
small runner, and a call state's once-per-call findings (the tsconfig bases a resolver cannot read)
would be reported once per instance.

## What stays identical, and how it is shown

- **Findings.** On the consumer's tree, `gate check --format json` before (released 0.3.0) and
  after (this branch's core and plugins, pinned with `gate lock --set` in a copy), cold: the output
  without timings is byte-identical, with the 85 expected findings. Run outside the repository.
- **Tests**, on synthetic trees only:
  - read-ahead: a working-tree and a commit snapshot give the same bytes with and without it;
    a file over the limit is still read; an unreadable file fails the plugin's read as before; a
    path read ahead is not recorded as asked; a cached-only run reads nothing ahead;
  - the symbolic-link check still refuses a file behind a linked directory after its sibling was
    read;
  - the TypeScript call state reads each file from the host once in a call whose rules parse it
    and then read its bytes (the existing read counter of `plugins/nestjs/tests/large`);
  - the ledger reads each file once per call, and its findings on the plugin's fixtures are
    unchanged;
  - each nestjs rule changed in step 6 keeps its fixture results; the registered-identifier scan
    gets a case with `-0`, `0x` and exponent forms;
  - the result cache: a call answered from the cache after a run with read-ahead hits, and an
    edited file read ahead misses.
- **Gates**: the lane's `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `cargo deny`.

## How it is measured

On the converted tree, under the load rule, with this branch's plugins pinned through
`gate lock --set` in a copy, the cold cache of the measuring script (pinned blobs only): the six
runs the lead asked for, (a) full check with history cold, (b) warm, (c) the reference tool,
(d) cold without history under `--warn-after 3 --fail-after 6`, (d2) warm after it, (e) the cold
pre-push against (c), each with `--timings` and its count of rules, in a private report outside
the repository.

## The lead's conditions

Approved with these conditions: steps 2 to 7 first, in that order of risk, then (d) measured; step 1
(read-ahead) only if (d) still counts over 4000 ms, and then only after this file says where the
core gets the paths to read ahead and what a wrong prediction costs, with a test that the result
cache key is the same with read-ahead on and off. Each step's own gain is recorded here, every plugin
whose wasm bytes change is listed with its growth, and `cargo deny` stays clean with the new
`regex-automata` features. No change to fuel, limits, the plugin interface or git. `--skip-rule` is
used in (d) only once T-100 is merged.

## Progress

- Written, not yet measured on the core: step 2 (`Directories` in `snapshot.rs`), steps 3 and 4
  (`TypeScriptCall` keeps parsed bytes; `Sources::contexts` shares one `owners::Sources` per root,
  used by the six rule sites), step 5 (the features), step 6 (the registered-identifier scan looks
  identifiers up in a set of the values' bits, with `-0` as `0`; raw literals are scanned as bytes;
  `asset-storage` keeps each file's tokens for the run), step 7 (`LedgerCall`: one catalog and one
  `Ledger::read` per call).
- A native run of the nestjs rules on the consumer's tree (private harness, never committed): host
  reads from 15265 to 10787 for 10786 distinct files, the same 301 findings per rule.

## Resume point

2026-11-23T07:04:00+00:00: steps 2 to 7 written; waiting for the lane's clippy pass, then one commit
per step and the wasm builds per step.

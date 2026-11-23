# T-098: a cold check without history under 6 s

| Field | Value |
| --- | --- |
| Status | approved (`3736fa66`); steps 2 to 7 written, tested and measured, gates green; each step's milliseconds waiting for the load |
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
   The refusal holds for the tree as first seen: a directory swapped for a link while the run
   reads the tree is followed on a later sibling read, where `main` refused it; such a swap is out
   of scope, as a file's contents changing mid-run are, since each path's contents are kept from
   their first read (review of `352c10fe`).
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

## Step 1, before it is written: the paths, and what a wrong guess costs

- **Where the paths come from.** In `calls()` of `plugins.rs`, after the result cache was asked
  and before the missed calls start: every tracked path of the run's snapshot (`Tree::paths`, the
  sorted list the listings already search) that the grant of at least one missed call allows
  (`Grant::allows`), in path order, each once. The core does not guess what a rule will read: it
  takes what the calls may read. Nothing comes from an earlier run or from the result cache, so the
  read-ahead depends only on the snapshot and the grants. A run whose calls all come from the
  result cache has no missed call and reads nothing ahead.
- **How they are read.** A few threads take paths from that list and read each one through the
  snapshot as `Snapshot::read` does (the same function, the same symbolic-link check, the same
  content cache), with one difference: nothing is recorded as asked. A path whose size (which the
  snapshot knows from the index for a clean file, without reading it) is over 1 MiB is skipped, and
  every path once 256 MiB were read ahead. A failed read is dropped, so the plugin's own read meets
  the error as today. The content cache keeps the first bytes read for a path, so every plugin of a
  run sees one content per path. The read-ahead stops when the calls end.
- **What a wrong guess costs.** A path the grants allow but no plugin reads costs one `open` and
  `read` on a read-ahead thread and its bytes in memory until the run ends, at most 256 MiB in
  all. It costs no correctness: the read set a call records, and so the result-cache key and what
  is stored under it, come from the plugin's own reads only. On a runner with few cores the
  read-ahead threads take CPU the calls could use, so their number follows the machine's
  parallelism.

## Progress

Each step's own gain, measured on the consumer's tree with the released 0.3.0 core and the plugin
alone (`gate check --format json --timings --plugin <name>=<wasm> --rule '<name>/*'`, results
emptied before the run), the wasm built as released (`scripts/build-component.sh`) at each step's
commit. Fuel is deterministic, so it is the measure while the machine's load is high; the
milliseconds of each step follow once the load is under 18.

| Step | Commit | Plugin | Fuel | Host reads | Change in fuel |
| --- | --- | --- | --- | --- | --- |
| base | `3736fa66` | nestjs | 57.04e9 | 15267 | |
| 3, 4: bytes kept, bindings shared | `688017a9` | nestjs | 53.58e9 | 10789 | -3.46e9 (-6.1 %) |
| 5: regex engines | `e0c0a0d1` | nestjs | 46.11e9 | 10789 | -7.46e9 (-13.1 %) |
| 6: registered identifiers, byte scan, tokens once | `ac0e1bc0` | nestjs | 43.91e9 | 10789 | -2.20e9 (-3.9 %) |
| 6: FxHash in `advisory-lock-boundary` | `9f0054fb` | nestjs | 41.81e9 | 10789 | -2.10e9 (-4.8 %) |
| 6: one interpreter per file, fragments read in place | `60a14694` | nestjs | 39.82e9 | 10789 | -2.00e9 (-4.8 %) |
| 6: import bindings from the text first (reverted by `4cd6ed3d`) | `b5f90e16` | nestjs | 40.09e9 | 10789 | +0.27e9 (+0.7 %) |
| 6: `module-registry` reads each chapter's source map once | `55d73220` | nestjs | 36.32e9 | 10789 | -3.50e9 (-8.8 %) from `60a14694` |
| 6: FxHash in `raw-config-access` | `542150f1` | nestjs | 35.35e9 | 10789 | -0.96e9 (-2.7 %) |
| 6: `asset-storage` tokens borrowed | `570c94a3` | nestjs | 34.82e9 | 10789 | -0.53e9 (-1.5 %) |
| 6: `advisory-lock-boundary` literal scan passes non-digits, UTF-8 checked once | `fbb2fa44` | nestjs | 34.31e9 | 10789 | -0.51e9 (-1.5 %) |
| 6: line index built on first use (`gate-typescript`) | `80518fe3` | nestjs | 34.07e9 | 10789 | -0.24e9 (-0.7 %) |
| base | `3736fa66` | ledger | 38.83e9 | 13020 | |
| 7: ledger call state | `59adf9a7` | ledger | 21.04e9 | 11585 | -17.79e9 (-45.8 %) |

The JSON findings of every step are byte-identical to the base's but for the summary line that
names the override. The ledger's remaining reads are `ledger/manifest`'s scan of every text file
for forbidden references, one read each. The ledger wasm is the same through step 6 and the nestjs
wasm the same at step 7.

Where `advisory-lock-boundary` spends its fuel, by ablation: the rule alone (`--rule
nestjs/advisory-lock-boundary`) at `9f0054fb` is 16.16e9, and a private build that skips one part
(never committed) gives that part's share: the executable-statement scan 5.91e9, the
registered-identifier inventory 3.98e9 (of it the numeric evaluation 1.62e9 and the SQL, JSON and
shell files 0.22e9), the shell commands 0.16e9, the mint check 0.13e9; the rest, about 6.0e9, is
reading and parsing its files and the raw-literal and recovery scans.

Each costly nestjs rule alone at `542150f1` (`--rule nestjs/<rule>`, results emptied; each count
holds the parse and semantic model of the files the rule reads, which the rules share in one call):
`advisory-lock-boundary` 14.07e9, `notification-egress` 6.23e9, `raw-config-access` 5.98e9,
`field-inventory` 5.71e9, `automator-metadata` 5.10e9, `asset-storage` 3.29e9, `barrel-exports`
3.19e9, `module-registry` 2.37e9, `advisory-lock-manifest` 1.48e9. Their sum, 47.4e9, is 12e9 over
the call's 35.35e9 with every rule, which is the shared parse counted once per rule; so
`advisory-lock-boundary` stays the rule to take next.

At the base's ratio (6572 ms for 57.0e9 fuel and 1111 ms of reads), nestjs at `60a14694` (39.82e9)
counts about 3.8 s before its reads, so the next rules of the fuel list are taken next; the
milliseconds follow at a load under 18.

## Measured

The six runs at `85f60fc3` (no plugin code changed since `80518fe3`): the core built with
`cargo build --release --locked -p gate` (gate 0.3.1), every plugin built as released and pinned
with `gate lock --set` in a copy of the consumer's tree, one run at a time through the lane,
2026-11-23T14:21:34+00:00 to 14:37:42+00:00, 1-minute load 16.35 to 20.03 (each run started under
18). Counted is the total of `--timings` less its compiling line; the findings were compared as
the JSON findings array (`jq -S`) without the `gate/budget` line.

| Run | Total | Compiling (exempt) | Counted | Findings |
| --- | --- | --- | --- | --- |
| (a) full check with history, cold | 16326 ms | 2117 ms | 14209 ms | the 85 |
| (b) the same, warm | 578 ms | 0 | 578 ms | the 85 |
| (c) the reference tool, full | 54.45 s | | 54.45 s | its own (0 errors) |
| (d) cold, no history rules, `--warn-after 3 --fail-after 6` | 9300 ms | 2422 ms | 6877 ms | the 85 and `gate/budget` |
| (d2) warm after (d) | 1016 ms | 0 | 1016 ms | the 85 |
| (e) cold pre-push of the consumer's last merge | 21028 ms | 2457 ms | 18571 ms | the 85 and 2 (below) |

(d) was then run cold seven times more, one after another: counted 4115, 4182, 4259, 4016, 4035,
4006 and 3114 ms (as the `gate/budget` line gives them). Over the eight runs the median is 4075 ms,
seven of eight within 3114 to 4259 ms. Which runs carried `gate/budget`, and at what level:

| Run | Counted | `gate/budget` |
| --- | --- | --- |
| 1, the (d) of the six | 6877 ms | error: over `--fail-after 6` |
| 2 | 4115 ms | warning: over `--warn-after 3` |
| 3 | 4182 ms | warning |
| 4 | 4259 ms | warning |
| 5 | 4016 ms | warning |
| 6 | 4035 ms | warning |
| 7 | 4006 ms | warning |
| 8 | 3114 ms | warning |

Only the first carried the error; every run carried the 85 findings besides. The untimed check run
before the six, at a load of 33 to 50, also carried the error (6723 ms). In every run the nestjs
call is the critical path at 34.07e9 fuel: 2843 to 3960 ms in the seven, 6209 ms in the first, with
the same fuel and the same 10789 reads (283 to 857 ms of them in the host). The machine has 6
faster and 12 slower cores (`sysctl hw.perflevel0.logicalcpu hw.perflevel1.logicalcpu`); the
first run's nestjs call took 1.6 to 2.2 times the others' with the same work, which a call kept on
a slower core would explain; which core it ran on was not recorded.
Against 0.3.0's counted 6973 ms (nestjs 6572 ms at 57.0e9), (d) counts 42 % less at the median.

(e) ran with the push range ending at a scratch commit in the copy that pins this branch's core and
plugins, because a push run reads the configuration of the pushed commit, and the consumer's pins
0.3.0. The 2 findings past the 85 are `git/main-history` on that scratch commit, which changes
`gate.toml` and `gate.lock` on main directly. Of its 18571 ms, the git call is 15490 ms (36.34e9
fuel; `git.sync-state` 6970 ms), unchanged by this task; (c), which is what the pre-push is held
to, took 54.45 s.

## Gates

At `85f60fc3`, through the lane in a clean detached worktree, 2026-11-23T14:38:14+00:00 to
14:54:30+00:00: `cargo build --release --locked` exit 0, `cargo fmt --all --check` exit 0,
`cargo clippy --locked --all-targets -- -D warnings` exit 0, `cargo test --locked` exit 0 (124 test
binaries ok), `cargo deny --locked check` exit 0. The commits after it change documents only.

## Plugin bytes

Every plugin built as released (`scripts/build-component.sh <plugin> wasm32-wasip2`, one cargo run
each) at `main` (`03ab4b7`) and at the branch (`b0756793`); `shasum -a 256` and `stat -f %z` of each
wasm. `git diff --stat b0756793 352c10fe` names only `docs/plan.md` and this file, so the bytes at
`352c10fe` are those at `b0756793`. This is the final list for the release: the plugins whose bytes
change, from this task's code and from the `regex-automata` features alike, are exactly these five:

| Plugin | sha256 at `main` | sha256 at the branch | Bytes at `main` | Bytes at the branch | Growth |
| --- | --- | --- | --- | --- | --- |
| cdk | `f4c170ef4eef797ba1a03a9883babab14de47a79be4f813717f8414e13fc8a49` | `03f150fa8287f5f26c35e1272f642bb58408b583c6984703244f9d5dd18554b4` | 1992818 | 1994786 | +1968 (+0.10 %) |
| frontend | `dc99a84f5edcd4ff293322753f2391060915657498f986b4d17b4225854c7b2f` | `4f0eaf93a7a67749dd14245ad9db8b5da3964f17bda757477e6d7b71d88c626a` | 2904217 | 3304234 | +400017 (+13.77 %) |
| ledger | `f1f6cfed330222ad697751afd98aa0a3eae007d978b051201d628e8a6bbd38c8` | `cd6ee588bd1c7fbfd0f9230761cfac4fcfebe0535b2611f48153a000e1ce68b6` | 2239701 | 2246402 | +6701 (+0.30 %) |
| nestjs | `d633c48ae6653b87a3e4c51d98b8cb4c8ec9d6dba1fa710b2d950fa3816933bc` | `4f1c15093470094b36696c7715841a227bb781ca5bc256b557fba4dbbdac3008` | 6296190 | 6715679 | +419489 (+6.66 %) |
| typescript | `f94cd3626b32e22990c34a4671d285a413fb7f1724d420022a75c254e6ff099f` | `b743834c519b6c291acf1d18de7f54225e51e9e9aae4609ce78458b1cb5bf29a` | 2749805 | 2754425 | +4620 (+0.17 %) |

The same bytes at both: `agents`, `ci`, `docker`, `docs`, `git`, `markdown`, `node`, `repository`.
`frontend` grows most: it uses the meta regex (`regex-automata`), which now carries the faster
engines of step 5, as `nestjs` and `ledger` do; `cdk`, `frontend`, `nestjs` and `typescript` also
build `gate-typescript`, whose bindings and line index changed.

## Resume point

2026-11-23T14:39:21+00:00: the six runs are in "Measured": (d) cold counts 4075 ms at the median of eight
(3115 to 6877 ms), under the 6 s bound, at the 4000 ms aim; findings are the 85 in every run. The five
workspace commands exit 0 at `85f60fc3`. Waiting: each step's build alone three times at a load
under 18 for its milliseconds (`steptime`). Step 1 (read-ahead) is a local draft only, out of the branch;
the nestjs call's own reads are 283 to 857 ms of it, so the lead decides whether it comes in.

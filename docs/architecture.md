# Architecture

Status: reviewed on 2026-11-16 (section 16); plugin form decided from measurements (section 3). This is
the design release 1 is built to.

## 1. What Gate is

Gate is a repository linter and gate. A repository declares its rules in `gate.toml` files; Gate reads
the repository's tracked files and its git history, runs the rules of the plugins the configuration
names, and reports findings. It also writes and verifies the files that wire it in (git hooks, CI jobs)
and runs a repository's verification steps as named suites.

The core is the tool. It never contains a rule, never names a language, framework or product, and never
assumes a layout. Everything that knows something about code is a plugin.

| The core does | Plugins do |
| --- | --- |
| configuration: load, merge, validate against each rule's schema | rules: look at anything the core exposes and report findings |
| projects: discover the projects of a repository and their roots | languages: parse files of one language, expose a library to rule authors |
| snapshots: the tracked tree of the working copy, the index or a commit | generators: produce files from the repository's data (agent rule adapters, tables) |
| change selection: changed paths, the projects they touch | presets: named rule sets with options (`recommended`, ...) |
| the plugin runtime: fetch, verify, cache, run, sandbox | |
| findings: exceptions, norms, levels, order, reports (text, JSON, SARIF, GitHub) | |
| cache of results, timings and budgets | |
| hooks and CI jobs: write them, verify them (`gate sync`) | |
| steps and suites: run external commands, pinned external tools | |
| `doctor`, `init`, installation of itself | |

## 2. Shape

```
gate (CLI)
 └─ gate-host ── configuration, projects, snapshot, git, change selection
       │        ── plugin runtime ── plugin A.wasm ── links languages/typescript (oxc)
       │                         ── plugin B.wasm ── links languages/typescript
       │                         ── plugin C.wasm ── (no language: git history, markdown)
       │        ── findings, exceptions, report, cache
       └─ sync (hooks, CI), run (steps, suites), doctor
```

A plugin is one file, `<name>.wasm`, the same for every platform. A plugin may hold many rules,
generators and presets. Plugins are independent: they share nothing at run time and are run in parallel.

## 3. Plugin form

**Decision: WebAssembly components (WASI 0.2), run by the core through an embedded `wasmtime`.**

Why this form:

- **One artifact for every platform**, pinned by one sha256. A product's plugin is built once in that
  product's release job and pinned like the core; no consumer worktree or CI job needs a compiler.
- **Sandboxed and deterministic by construction**: a plugin sees only what the host gives it (the
  tracked tree of its scope, read-only; git data through host calls; its options). No file writes, no
  network, no clock, no environment. The host knows every input a plugin read, which makes result
  caching exact (section 9). A component built with Rust's std also imports `wasi:cli`, `wasi:io`
  and `wasi:clocks` 0.2; the runtime links these to fixed stubs: an empty environment and argument
  list, no preopened directory, a constant clock, and stdout and stderr captured into the run log.
- **Rules in any language that targets components**; Rust first-class through the SDK.
- **No speed cost that matters** (T-001, below).

Measured (T-001, 2026-11-16; [spike/RESULTS.md](../spike/RESULTS.md) holds every command): a tree of
9,881 TypeScript files (34.2 MB); the same Rust source built native and as a `wasm32-wasip2` component
under wasmtime 50, the tree read through host calls; 5 to 8 runs per row at a machine load of 48 to 137
on 18 cores. Instructions retired are not load-stable on a shared machine (they vary by about 0.5 G
between runs), so rows are medians and a rule's cost is the workload's own phase clock, minimum over
runs.

| Workload (medians) | G instructions native / wasm | CPU s native / wasm | Max RSS MB native / wasm |
| --- | --- | --- | --- |
| read and parse every file (oxc) | 7.06 / 8.02 | 1.73 / 1.71 | 226 / 175 |
| parse, walk, semantic model, module resolution | 7.71 / 9.08 | 1.42 / 1.52 | 241 / 185 |
| three plugins at once, each parsing the tree (wall s) | 3.9 / 4.7 | 3.29 / 4.46 | 705 / 528 |

| Phase (minimum ms) | Native | Wasm | Ratio |
| --- | --- | --- | --- |
| read 9,881 files | 630 | 693 | 1.10 |
| oxc parse | 200 | 329 | 1.65 |
| full AST walk | 28 | 26 | 0.94 |
| scopes, symbols, references | 32 | 37 | 1.16 |
| resolve every import specifier and the cycles | 107 | 132 | 1.24 |

Parse plus the three rules is about 1.0 s native and 1.2 s as a component. A component compiles in 383 ms once; loading the
cached compiled module takes 0.9 ms and instantiation 1.3 ms. Reading through a WASI preopened
directory costs 3.8x the instructions of a host-call read, so plugins get no file system: the tree is
reached only through `tree.read`. On a one-file staged change a full component re-run takes 2.1 s CPU;
the hook's speed bound rests on the read-set result cache (section 9).

Alternatives weighed and not taken:

| Form | Why not |
| --- | --- |
| native process per plugin over a small protocol | one binary per platform per plugin, no sandbox, cache keys only by declaration; no speed gain to pay for it |
| native dynamic library | unsafe ABI, per platform, no sandbox |
| declarative rules only (queries over syntax trees) | cannot express cross-file graphs, ledgers, git rules; may come later as one plugin that runs queries |

## 4. Plugin interface

The interface is a WIT package, `gate:plugin@0.2.0`, in [`wit/`](../wit). Sketch:

```wit
package gate:plugin@0.2.0;

interface types {
  enum level { error, warning, note }
  enum scope { repository, project }
  record span { line: u32, column: u32, end-line: u32, end-column: u32 }
  record file-location { path: string, span: option<span> }
  variant location { file(file-location), commit(string) }   // a commit finding names the full id
  record finding {
    rule: string,             // the rule's name inside the plugin: "barrel-exports"
    aspect: string,           // a named part of the rule; "" when the rule has one
    message: string,
    location: option<location>,
    target: option<string>,   // what the finding is about, in the rule's own documented grammar
                              // (for example "src/user.entity.ts:UserEntity.parent"); exceptions match it
    help: option<string>,
    key: option<string>,      // the plugin's stable identity of what the finding is about; with
                              // it, the fingerprint is rule, aspect, path and key
  }
  record rule-info {
    name: string, title: string, aspects: list<string>, scope: scope,
    reads-history: bool,      // needs commits, not only the snapshot
    languages: list<string>,  // e.g. ["typescript"]; empty for language-free rules
    options-schema: string,   // JSON Schema of the rule's options; its "x-gate-needs" lists the
                              // [plugins.<name>] options the rule reads that the plugin's schema leaves optional
  }
  record generator-info { name: string, title: string, options-schema: string, owns: list<string> }
  record preset { name: string, rules: string /* JSON: rule name -> {level, levels, options} */ }
  record plugin-info {
    name: string, version: string,
    options-schema: string,   // JSON Schema of the [plugins.<name>] options
    rules: list<rule-info>, generators: list<generator-info>, presets: list<preset>,
  }
  record rule-run {
    name: string,
    options: string,                // JSON, validated by the host against the rule's schema
    options-file: option<string>,   // the tracked file the options came from, readable by the rule
    exceptions: list<string>,       // the targets of this rule's exceptions, for rules whose result depends on them
  }
  record check-request {
    project: string,
    rules: list<rule-run>,
    plugin-options: string,         // JSON: [plugins.<name>] options, shared by the plugin's rules
    languages: string,              // JSON: [languages.<id>] settings of the languages the plugin declares
    norms: list<string>,            // every norm the configuration maps, for rules about rule documents
  }
  record generated-file { path: string, contents: list<u8> }
  record generate-result { files: list<generated-file>, findings: list<finding> }
  variant plugin-error {      // why check or generate failed; the message says what to do
    configuration(string),    // the options or settings cannot be used: a configuration error
    environment(string),      // anything else outside the configuration: an environment error
  }
}

interface tree {      // the snapshot, read-only, tracked files only
  paths: func(prefix: string) -> list<string>;             // paths under a prefix, sorted
  read: func(path: string) -> option<list<u8>>;            // none when not tracked or not granted
  changed: func() -> option<list<string>>;                 // the change set, when the run has one
}

interface git {       // granted only to rules that declare reads-history
  record commit { id: string, parents: list<string>, message: string, author-date: string }
                                                           // message is the raw full message; the SDK
                                                           // derives subject and body from it
  record commit-range { base: option<string>, head: string, target-ref: option<string> }
                                                           // base none for a new branch; target-ref
                                                           // the pushed remote ref under --push
  head: func() -> option<string>;
  branch: func() -> option<string>;
  range: func() -> option<commit-range>;                   // the run's range under --rev or --push
  resolve: func(rev: string) -> option<string>;            // a ref or revision to a commit id
  tree: func(commit: string) -> string;                    // the commit's tree id
  merge-tree: func(first: string, second: string) -> option<string>;
                                                           // tree of git's conflict-free merge, none
                                                           // on conflict
  log: func(range: string) -> list<commit>;
  changed-paths: func(parent: string, commit: string) -> list<string>;
  read-at: func(commit: string, path: string) -> option<list<u8>>;
  is-ancestor: func(ancestor: string, descendant: string) -> bool;
  sync-state: func(commit: string) -> sync-state;         // what gate sync produces at that commit,
                                                           // and which of it differs
  message: func() -> option<string>;                       // the --commit-message file's text
  hook: func() -> bool;                                    // a git hook runs the check (--hook)
}

world plugin {
  import tree; import git; import log: func(message: string);
  export describe: func() -> types.plugin-info;
  export check: func(request: types.check-request) -> result<list<types.finding>, types.plugin-error>;
  export generate: func(name: string, project: string, options: string)
      -> result<types.generate-result, types.plugin-error>;   // findings surface as
                                                               // gate/sync#<generator>
}
```

Rules:

- One `check` call per plugin and project carries every enabled rule of that plugin, so a plugin
  parses a file once for all its rules.
- A `project` scope rule sees its project's subtree (paths relative to the project root). A
  `repository` scope rule runs once and sees the whole repository. A configuration may grant a plugin
  extra read paths outside its project (`reads = [...]`), for example a ledger that cites records of a
  sibling project; anything else returns `none`.
- `Err` from `check` or `generate` is a `plugin-error`: `configuration` when the options or settings
  the call was given cannot be used, which fails the run as a configuration error, and `environment`
  otherwise, which fails it as an environment error. The message names the plugin and, for a project
  below the repository root, the project and that its paths are relative to its root, since the
  core does not rewrite the plugin's text. A violation is a finding.
- The SDK (`gate-sdk`, Rust) wraps the interface: a `Tree` with `paths()`, `under()`, `text()`,
  `bytes()`; a `Git`; a `Report` with `finding(aspect, location, message)` (the level is configuration,
  never the plugin's); typed options with
  `serde`, for each rule, each generator and the plugin itself (`[plugins.<name>] options`, one type
  the plugin declares, `NoOptions` when it has none); a `plugin!` macro that writes `describe` from
  those declarations, each options type's JSON Schema included.
- `gate-test` runs a plugin against a fixture tree and compares findings with an expected file; every
  rule ships its pass and fail fixtures.

## 5. Languages

A language is a plugin too, in two parts:

- **The language library** (`languages/typescript`): parsing (oxc), module resolution (tsconfig paths,
  package exports), source owner tables, value folding; none of it is in the core.
  Rule plugins that look at TypeScript link it; it is versioned and released on its own, and a rule
  plugin pins the version it was built with.
- **The language plugin** (`plugins/typescript`): declares the language (`typescript`: `.ts`, `.tsx`,
  `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`) and its own rules (every tracked source parses; the
  syntax rules shared by every TypeScript repository).

The core knows only that a rule declares `languages = ["typescript"]`, which lets it skip a plugin for
a project or a change with no file of that language. A new language is a new library and plugin; the
core does not change.

## 6. Rules and findings

- A rule's id is `<plugin>/<rule>`, lowercase kebab: `nestjs/barrel-exports`, `git/no-ci-skip`,
  `ledger/record-status`. A rule may report several aspects (`nestjs/field-inventory#hidden`).
- A finding: rule id, aspect, level, message, location (path, line and column span), help, an
  optional key, the norm it enforces (from configuration), and a fingerprint for baselines and SARIF
  (`partialFingerprints."gate/v1"`).
- The key is the plugin's short, stable identity of what the finding is about within its rule,
  aspect and path (`ProviderEntity.name`, a controller's class name); it must not change when the
  message's wording does. The fingerprint is a digest of (rule, aspect, path, key) when the finding
  has a key (each part length-prefixed, after the word `key`), otherwise of (rule, aspect, path,
  message), so a keyed message can be reworded, or name a path differently, without breaking a
  baseline. A plugin gives a key whenever its message embeds a
  path or a value from the options.
- Levels: `error` fails the gate, `warning` reports, `note` informs; `off` disables. The level is
  configuration, never the plugin's choice.
- Norms: a repository maps a rule or an aspect to its own written rule (`norm = "RULE-IMP-001"`); the
  report and `gate explain` print it.
- Order is deterministic: path, line, column, rule, aspect, message.
- Exceptions are one model owned by the core (section 8): a finding that matches an exception is not
  reported, and an exception that matches nothing is itself reported, so exceptions only shrink.

## 7. Configuration

Two kinds of `gate.toml`: one at the repository root (repository-wide settings and rules), one in each
project (that project's plugins and rules). A project's file travels with the project when it moves
between repositories.

Root:

```toml
#:schema https://github.com/ElsiKora/Gate/releases/download/v0.2.0/gate.schema.json

[gate]
version = "0.2.0"                     # the pinned core; checksums in gate.lock

[workspace]
projects = "pnpm"                     # discover from pnpm-workspace.yaml; or a list of globs
ignore = ["samples/**"]

[plugins.git]
source = "ElsiKora/Gate:git@0.2.0"
options = { main = ["refs/remotes/origin/main"], activation = "<the output of git rev-parse HEAD>", release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$' }

[plugins.markdown]
source = "ElsiKora/Gate:markdown@0.2.0"

[rules]
"git/no-ci-skip" = { level = "error", norm = "RULE-GIT-001" }
"git/branch-name" = { options = { template = ":type/:ticket-:name" } }
"git/commit-message" = { levels = { header-min-length = "warning", scope-max-length = "warning" } }

[hooks]
pre-commit = ["check --staged"]
commit-msg = ["check --commit-message"]
pre-push = ["check --push"]

[ci.github]
workflow = ".github/workflows/gate.yml"
jobs.gate = { name = "Gate", run = ["check --changed --base origin/main"] }
aggregate = "required"  # one more job: needs every job, fails unless each succeeded or was skipped
```

`activation` is the commit the git rules start after: for a new policy, the output of
`git rev-parse HEAD`.

Project (`apps/server/gate.toml`):

```toml
[project]
name = "server"
extends = ["nestjs/recommended", "ledger/recommended"]

[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.2.0" }
nestjs = { source = "ElsiKora/Gate:nestjs@0.2.0", options-file = "gate/options/nestjs.json" }
ledger = { source = "ElsiKora/Gate:ledger@0.2.0" }
agents = { source = "ElsiKora/Gate:agents@0.2.0" }

[languages.typescript]
tsconfig = "tsconfig.json"

[rules]
"nestjs/import-direction" = { level = "error", norm = "RULE-IMP-001", options = { roots = ["src", "infrastructure/cdk"] } }
"nestjs/field-inventory" = { options-file = "gate/options/field-inventory.json" }
"nestjs/barrel-exports" = { level = "error", norm = "RULE-FO-003", norms = { wildcard = "RULE-FO-004" }, levels = { wildcard = "warning" } }
"ledger/record-status" = { options = { open = ["planned", "in progress"], closed = ["done", "dropped"] } }

[generate]
"agents/adapters" = { options = { rules = "docs/agents/rules", out = ".claude" } }

[steps]
lint = { run = "pnpm eslint ." }
unit = { run = "pnpm vitest run" }

[suites]
static = ["lint"]
all = ["lint", "unit"]
```

- **A plugin entry** is a table: `[plugins.<name>]` with `source`, and optionally `options` or
  `options-file` (settings shared by all of the plugin's rules, such as a source layout) and `reads`
  (extra repository paths it may read, relative to the repository root). The options belong to the
  plugin, not to its rules: they are validated against the schema the plugin declares wherever the
  plugin is declared, whatever its rules' levels, and an entry without options is validated as `{}`.
- **Language settings** are configured once per project in `[languages.<id>]` (for TypeScript: the
  tsconfig path, folders to skip) and passed to every plugin that declares the language.
- **Exceptions match by target**: an exception's `target` equals the finding's `target`, or its path
  when the finding has no target. An exception may name an `owner` (a record the ledger plugin checks
  is open); each rule's page documents its target grammar.
- **Plugin sources** have one form: `<owner>/<repo>:<plugin>@<version>`. It resolves by convention to
  the asset `<plugin>.wasm` of the GitHub release tagged `plugin-<plugin>-v<version>` of that
  repository; `gate.lock` records the URL and sha256.
- **What the root file may configure**: repository-scope rules in `[rules]`, and project settings for
  sets of projects in `[[profiles]]` (section 16). A project-scope rule in the root `[rules]` is an error
  that points to profiles.
- **A single-project repository** has no `[workspace]`: its root is its only project, so its root
  `gate.toml` also holds `[project]`, `[plugins]` and project-scope rules. The same holds when a
  workspace lists `"."` as a project.
- **Aspects** take their own norm and level inside the rule's entry: `norms = { <aspect> = "..." }`,
  `levels = { <aspect> = "..." }`; the rule's `norm` and `level` apply to the others. A preset may
  set aspect levels too. The project's configuration wins over a preset: an aspect's level is, first
  found, the entry's level of the aspect, the entry's level, the family's, the presets' level of
  the aspect, the presets' level of the rule.
- **Exceptions** live in one file beside the `gate.toml` they belong to, `gate.exceptions.toml`:

  ```toml
  [[exception]]
  rule = "nestjs/typeorm-relation#nullable"
  target = "src/modules/user/user.entity.ts:UserEntity.parent"
  reason = "historic column, removed by the migration of 2026-12"
  decision = "D-ORM-014"
  ```

  `[exception-policy]` in `gate.toml` says whether a decision is mandatory.
  There is no inline suppression comment: an exception is always an entry here, so every one has a
  reason, is listed by `gate list`, and is reported by `gate/exceptions` when unused or malformed.
- Every section is validated: unknown keys, unknown rules, `[generate]` keys that name no generator of
  their plugin, `extends` entries that name no preset of their plugin, options that fail the rule's
  schema, `[plugins.<name>] options` that fail the schema the plugin declares (a missing required
  option, an unknown one, a wrong type), and exceptions without a reason are configuration errors
  that name the file and key, raised before anything runs, in every run, whatever `--rule` and the
  change selection leave to run.
- A plugin option an enabled rule declares it reads (`NEEDS` in the SDK) missing from
  `[plugins.<name>] options` is a configuration error too, raised before anything runs, for the
  rules `--rule` leaves to run: a need belongs to the rules that run, so it follows `--rule`, and
  does so again when a `sync-state` answer checks the run's configuration. A plugin option only
  some rules read stays optional in the plugin's schema; the rule that reads it declares it, with
  the options it reads it under, so the error names that rule.
- A rule that is `off` in every aspect needs no options, so a rule a preset enables can always be turned
  off; options it does carry are still validated against its schema.
- The core's own rules (`gate/*`) take `norm` and `norms` like any rule; their level is fixed, and
  `level` or `off` on one is a configuration error.
- A project may turn a family off visibly: `"ledger/*" = "off"`; `gate list` prints what is off and
  where.
- Large option data sits in files (`options-file`), validated against the same schema.
- `gate.lock` (written by `gate lock`, committed) holds the resolved URL and sha256 of the core for each
  platform and of every plugin. A run with a lock that does not match the configuration fails.

## 8. Projects, snapshots and changes

- **Projects** come from the package manager (`pnpm`, `npm`/`yarn` workspaces, `cargo`, `go`), from
  explicit globs, or from `gate.toml` files found among tracked files. A path belongs to the project with
  the longest root that contains it; paths of no project belong to the root.
- **Snapshots**: the working tree of tracked files (default), the index (`--staged`), or a commit
  (`--rev <oid>`). Only git-tracked paths exist; nested worktrees and ignored files are never seen.
  Content hashes are git blob ids.
- **Change selection**: `--changed --base <rev>`, `--staged`, `--push` (the pushed range). The changed
  paths select the projects that contain them and the workspace projects that depend on those (from the
  package manager's workspace graph). Repository-scope rules run when any path changed. The report
  says which projects ran and which were skipped and why.
- **History**: rules that declare `reads-history` get the `git` interface; in `--rev` and `--push`
  modes the host passes the range under review. The branch it gives is `$GATE_BRANCH` when set (CI,
  where a pull request's checkout is a detached `HEAD`), else the branch `HEAD` is on, read from the
  full ref.

## 9. Execution and cache

- Plugin modules are compiled once per (plugin sha256, wasmtime version) and stored in the cache;
  later runs load the compiled module.
- A check runs only the plugins declared in a scope the change selection runs that `--rule`
  allows; only those are fetched and compiled. Validation is never narrowed: every run checks the
  whole configuration against what every declared plugin declares. What a plugin declares (its
  rules with their scopes, aspects and options schemas, its presets, its option schema and its
  generators) is stored in the cache whenever the plugin loads, under the plugin's sha256, the
  plugin interface and the core version. A plugin the run does not run answers from that entry,
  and is loaded for it when the cache holds none. A `sync-state` answer that falls back to the
  run's configuration loads the plugins of its `[generate]` entries when it first needs them.
  A stored declaration is trusted as a compiled module and a stored result are: what the cache
  holds decides what a run checks, so a cache directory shared between users or machines must be
  as trusted as the checkout.
- `gate sync` loads every declared plugin. It is the command every clone runs before its first
  hook, since it sets `core.hooksPath`, so after it every plugin is in the module cache and its
  declaration is stored, and no check compiles a plugin it does not run.
- Known cost: a `sync-state` answer computed at a commit's own configuration loads every plugin
  that commit pins.
- Plugin calls (project x plugin) run in parallel on a worker pool.
- **Result cache**: the host records every path a call listed or read and every git query it made.
  A call's findings are stored under (core version, plugin sha256, rules and options, project, grant)
  with that record, and a call is not run while every recorded input answers as it did: each listed
  prefix lists the same tracked paths, each read path has the same blob id, the change selection is
  the same, and each git question that goes through a ref answers the same commit ids. A `sync-state`
  answer is recorded by its digest and asked again, since an answer at a commit whose own
  configuration this core does not read follows the run's configuration. A generator's output (its
  files and findings) is stored the same way, under (core version, plugin sha256, generator,
  options, grant), and checked against the tree it would run on: `gate sync`, and `sync-state` at a
  commit, run only the generators whose read files changed. Exact, because a plugin cannot read
  anything else.
- **`sync-state` at a commit** answers two path lists: `produced`, every path `gate sync` writes or owns
  there (generator outputs, tracked files under an owned glob, the core's hook and CI files), and
  `drift`, the produced paths that differ from it. A history rule tells a generated file from a
  hand-written one only through it; no rule keeps its own list of generated paths. It runs the generators on that commit's tree with that commit's
  configuration. When the commit's `gate.toml` is not a configuration of this core (every commit before
  the repository adopted Gate, or one that fails validation, plugin options that break their plugin's
  schema included), it uses the run's own configuration and generators for `produced` and
  answers no `drift`: files another tool wrote cannot be judged by this core's generators, so on such
  a commit a history rule tells a generated file from a hand-written one by its path alone. That
  answer is cached under the commit id and the run configuration's hash. A generator's error at a
  commit never fails the call: it answers the paths it could not produce, and the answer is stored
  only when the error is lasting (the component does not load under this core, or the generator
  answers the same way on every run). A pinned plugin that cannot be fetched or does not match its
  pin, a failure of git, and a passing failure (the machine, a time limit) fail the call as an
  environment error rather than answer.
- **Time limits count the plugin's own work.** A call's deadline and fuel bound the time and work
  spent in the plugin; time spent in host calls it makes (`git`, `sync-state`) does not count against
  it. Fuel is the limit that decides: it is the same on every machine, so running out of it is a
  lasting failure. The deadline is only a backstop for a machine that stalls: on the machine the
  defaults were measured on, the default fuel (10^12) runs out first, at about 355 s of the 600 s;
  on a machine more than 1.7 times slower, the deadline can come first, and that failure may pass.
- **History is immutable, so its answers are cached for good.** The host answers `git` calls
  natively. `tree`, `changed-paths` (a diff of two trees, read one level at a time), `read-at` and
  `is-ancestor` (a walk of the commit graph by generation, the ancestry loaded with one `git
  rev-list`, left to git past 200 000 commits) go through one `git cat-file --batch` reader per run,
  not one git process per question, and `merge-tree` through one `git merge-tree --stdin`. Each
  answer about full commit ids (`tree`, `changed-paths`, `merge-tree` with git's version,
  `is-ancestor` outside a shallow clone, `read-at` as the id of the blob, whose content is stored
  once by that id) is asked once per run and stored, except a no or a missing file at a commit the
  clone lacks, which a fetch can change: a run writes what it learned to one file under
  `git/answers/`, every later run reads them back, and the files are merged into one when they
  pass sixteen. `sync-state` answers are stored by commit id. A call's
  findings are stored with the read set that proves them (see the result cache above): answers that
  depend on refs (`head`, `branch`, `range`, `resolve`, `log` of a range with a ref) are recorded
  there by the commit ids they returned, so a moved ref re-runs the call, and the re-run asks git
  only about the new commits.
- The cache lives outside the repository (`$GATE_CACHE_DIR`, default the user cache directory),
  content-addressed, shared by every worktree, filled by atomic rename, safe under concurrent first use,
  and needs no network once filled.
- `--timings` prints the time of each phase, of each plugin call and whether it ran or came from the
  cache, the fuel a call used (the same on every machine, so it compares runs on a loaded one), the
  host's share of a call by kind of host call (`git.sync-state`, `tree.read`), and, for a call of
  more than one rule, the time and fuel of each invocation with its rules (a call's snapshot rules
  share one instance, so `--rule <plugin>/<rule>` gives one rule's own); then how many `sync-state`
  answers the run computed and read from the cache, how many git questions about commit ids it
  asked git and answered from stored answers, and how many generators ran and came from the cache;
  `--budget <seconds>` warns when a run exceeds it and `--budget-fails` makes that an error (the
  generated CI jobs pass it, hooks do not); `--fail-fast` stops at the first error.
- `--rule <id>` (repeatable, `<plugin>/*` allowed) narrows a run to those rules; exceptions and
  baseline entries of the other rules are not due in that run. There is no history switch: a history
  rule is turned off by its level, and its git answers are cached by commit id.

## 10. Generated files

- **The core's own**: git hooks (`.githooks/*`, `core.hooksPath` set by `gate sync`) and CI workflow
  jobs, from `[hooks]` and `[ci]`. With `[ci.github] aggregate = "<id>"` the workflow has one more
  job that needs every other job, runs `if: ${{ always() }}` and fails unless each succeeded or was
  skipped: the single check branch protection requires. It runs on `[ci.github] runs-on` and needs
  `bash` and `jq` there. Only the `bin/gate` steps get `GATE_GITHUB_TOKEN` (the repository secret of
  that name, else the job's token) and `GATE_BRANCH` (`github.head_ref || github.ref_name`), as step
  environment, never written into a command; actions and other steps get neither, and `gate run`
  gives a step neither `GATE_GITHUB_TOKEN` nor `GITHUB_TOKEN` unless its own `env` sets it. The
  hooks call the gate, never skip themselves on an environment variable, and in a monorepo run only
  the projects the change touches.
- **Plugins'**: a generator returns files (for example the `AGENTS.md` block and the `.claude/rules` and
  `.claude/skills` adapters, scoped to the project's path; a ledger's review table). Its paths are
  relative to the project root, like a rule's; the host maps them to repository paths. A generator
  declares the globs it owns (`generator-info.owns`); `gate sync` deletes a tracked file under an owned
  glob that no generator produced, and `gate sync --check` reports it, so stale output never needs a
  rule of its own.
- `gate sync` writes every generated file; `gate sync --check` reports drift as findings of the
  pseudo-rule `gate/sync`, which the core owns because the files are its own output.
  Facts that the generated files make true by construction (hooks call the gate, the gate job runs
  every suite step) are not rules; rules about workflow files a repository writes by hand live in the
  `ci` plugin.

## 11. Steps, suites and tools

- `[steps.<name>]`: a command, its working directory (the project), environment, and timeout.
  `[suites]`: ordered lists of steps. `gate run <suite or step>` runs them and reports each exit code.
- `[tools.<name>]`: an external tool pinned by version and sha256 per platform, fetched and verified
  with the same code that fetches plugins (its URL is `https`, or plain `http` to this machine only,
  as in `gate lock`). A step names the tools it calls in `tools = ["<name>", ...]`, and that list is
  the one rule: `gate run` installs a step's tools just before the step runs (a set once per run; a
  skipped step, or one fail-fast stops, installs nothing), and each step's `PATH` starts with its own
  tools only; the command's words decide nothing. A step that names
  a tool the root does not declare is a configuration error at load.
- A step's `$1`, `$2`, ... are the arguments of `gate run <name> -- <argument>...`, and its standard
  input is what `gate run --stdin` read, else empty. A hook's `run` entry passes the hook's arguments
  (`-- "$@"`: the message file of `commit-msg`) and, in `pre-push`, the pushed refs (`--stdin`).

## 12. Distribution

- **The core**: GitHub release `v<version>` of `ElsiKora/Gate` with `gate-<target>.tar.gz` for
  `x86_64-unknown-linux-musl` and `aarch64-apple-darwin`, and `SHA256SUMS`.
- **A shared plugin**: GitHub release `plugin-<name>-v<version>` with `<name>.wasm` and its sha256.
  Each plugin has its own version and changelog.
- **A product's plugin**: the same, as a release of the product's own repository; pinned the same way
  (`<owner>/<repo>:<plugin>@<version>`). Built in that repository's release job; a change is
  proven before release by an explicit, command-line-only override (section 16).
- **One way to install**: `gate init` writes `bin/gate`, a short POSIX launcher committed to the
  repository. It reads the core's version and sha256 from `gate.lock`, downloads the archive once into
  the cache, verifies it, and runs it. CI and local machines use the same launcher. The core fetches and
  verifies plugins the same way. A checksum mismatch fails closed and deletes nothing it did not write.
  Every download, by the core or by the launcher, has 30 s to connect, 30 s for each read, and 15
  minutes in all, shared by its redirect hops: a server that stops sending fails it with an error
  that names the URL and says to try again, and nothing is retried. The launcher's wget fallback has
  the connect and read bounds but no overall limit, which wget does not offer; curl, which the
  launcher prefers, has all three.
- **Reproducible**: every component, the core and each plugin, is built alone (one package per cargo
  invocation) by `scripts/build-component.sh`, which resolves the checkout, the target directory and
  `CARGO_HOME` to physical paths and remaps them, sets the rustflags itself and drops the caller's
  profile overrides, and, for Apple targets, links without debug information; the core's archive is
  packed by `scripts/pack-core.sh` with a fixed mtime and owner. Its sha256 then depends only on the
  source, `Cargo.lock`, the toolchain of `rust-toolchain.toml`, the build host and the runner image:
  the C compiler that builds `ring`, the linker, `tar` and `gzip` are part of the bytes, so a rebuild
  needs the same image (`ubuntu-24.04` for `x86_64-unknown-linux-musl`, `macos-15` for
  `aarch64-apple-darwin`). Plugins are released from `ubuntu-24.04`: in run 44307223391 every plugin
  module differed between the Linux and the macOS host while each host rebuilt its own to the same
  sha256, so a plugin's pin is reproducible on an x86_64 Linux host with that runner image. Both
  release workflows build through `reproducible.yml`, which builds each component twice, in a second
  path with a second cargo home, and fails unless both builds have the same sha256; only then is the
  first build released. It also runs on every push to `main` and to a task branch: on each runner for
  its core, and on `ubuntu-24.04` for every plugin.

## 13. This repository

```
crates/gate          the command line
crates/gate-host     the core: configuration, projects, snapshot, git, cache, report, sync, run
crates/gate-runtime  the plugin runtime: wasmtime, the host side of the interface, compiled-module cache
crates/gate-sdk      the Rust SDK for plugin authors
crates/gate-test     the fixture harness for plugins
wit/                 the plugin interface
languages/typescript the TypeScript language library
plugins/typescript   the TypeScript language plugin
plugins/git          commit, branch and history policy
plugins/ci           CI workflow files: actions pinned by commit, workflow triggers that resolve
plugins/markdown     documentation links and sections
plugins/agents       agent rule files and their generated adapters
plugins/ledger       decision ledger records, statuses, closers; the review table generator
plugins/nestjs       NestJS services on the CRUD automator, including egress notification
docs/                guides: getting started, configuration, writing a plugin, writing a rule
scripts/             the release build of a component and the core's archive
```

Plugins of one product live in that product's repository.

## 14. One configuration format

Gate's own files are `gate.toml`, `gate.exceptions.toml`, `gate.lock`, `gate/baseline.json` and the
options-files they name; to find projects it also reads the package manager's workspace lists. It
reads no other tool's configuration.

## 15. Answers to the design requirements of the first review

| Requirement | Answer |
| --- | --- |
| How a product's plugin is built and pinned | a `.wasm` built in the product's release job, pinned by sha256 like the core; no compiler in worktrees or CI jobs that run the gate |
| `generate` belongs to plugins | generators are plugin exports; the core writes and verifies their output |
| A common check shared by products | it lives in a shared plugin (`plugins/nestjs`); products enable it |
| Roots hard-coded to `src/` | source roots are rule options; projects give each its own root |
| The policy file format | rule data are options validated by the rule's schema (unknown keys fail); exemptions are core exceptions with a reason and an optional decision checked by the ledger plugin |
| One status vocabulary | `ledger/record-status` options |
| Records of another product | a plugin may be granted read paths outside its project |
| Hook shape owned by the tool | the core generates the hooks; they never skip on `CI` |
| Rule fixtures | rules are tested against `gate-test` fixture trees |

## 16. Decisions from the first review (2026-11-16)

The first readers raised these points; each is settled:

**Configuration**
- **Profiles.** The root `gate.toml` may hold `[[profiles]]`, each with `projects = [globs]` and any
  project setting (`plugins`, `extends`, `rules`, `steps`, `suites`, `generate`). A project gets, in
  order: the root's repository defaults, every matching profile, its own `gate.toml`. `gate list
  --origin` prints the file and section each setting came from. A project with no file of its own is
  fully described by profiles.
- **Root steps and suites.** `[steps]` and `[suites]` are allowed at the root and run with the
  repository top as working directory. A suite runs every step and prints each exit code and time, then
  fails if any failed (`fail-fast = true` stops at the first). A step has `timeout`. A step marked
  `heavy = true` is prefixed by the command in `[run] heavy-wrapper` or `$GATE_HEAVY_WRAPPER` (for a
  machine-wide job queue); nothing is hard-coded.
- **Staged files for steps.** A step may declare `files = "staged"` (or `"changed"`) and `glob`; the
  matching paths are appended to its command, so formatting and linting of staged files need no extra
  tool. A hook may also run any step.

**Plugins**
- **Local development of a plugin.** `gate check` and `gate sync` take `--plugin <name>=<path.wasm>`,
  which replaces one plugin for that run only. The plugin it names need not be pinned yet: the lock check skips every plugin an
  override covers and still requires the others to be pinned and to match. It exists only on the command line (never in a file), the report names the
  override and its sha256 on every line of the summary, its results never enter the shared cache, and
  `gate lock` refuses to run with one. A product's CI may use it in the job that proves a changed
  plugin on its own tree; the generated gate jobs pass `--no-overrides`, which refuses it. Only a
  plugin author needs a toolchain (Rust with the `wasm32-wasip2` target and `gate-sdk`); consumers never
  do.
- **Releasing a product's plugin in one commit.** The release job builds the plugin (reproducibly),
  computes its sha256, runs `gate lock --set <name>=<release asset URL> --sha256 <hash>`, commits the
  lock in the release commit, tags it and uploads the same file as the tag's asset. The pin moves in
  the release commit itself.
- **Private assets.** The launcher and the core fetch only the URLs written in `gate.lock`. For a private
  repository they authenticate with, in order, `$GATE_GITHUB_TOKEN`, `$GITHUB_TOKEN`, `gh auth token`.
  A missing or insufficient credential fails closed (exit 2) with a message naming the asset, the URL
  and the variable to set. `ElsiKora/Gate` itself is public from `v0.1.0`, so its core and shared
  plugins need no credential.
- **No clock, no environment.** A rule reads no date, environment or untracked file: a date is an
  option or the commit date from `git`.

**Generated files**
- **Output paths are relative to the project.** A generator writes where its options say, inside its
  project; the core maps each path into the repository and refuses one outside the project or one it
  owns. A project's agent adapters go to the project's own `.claude/rules/` and `.claude/skills/`, and
  its `AGENTS.md` block to the project's own `AGENTS.md`. A file no generator produces is never
  written.
- **Hooks.** `core.hooksPath` is set once per clone and shared by its worktrees; a fresh worktree needs
  nothing. `gate doctor` says when a clone has not run `gate sync`. Hook entries are gate commands,
  including `run <step>`, so commit message and branch linters and staged formatting stay steps until
  `git/*` rules replace them. `[hooks] budget` applies `--budget` to hook runs.
- **CI jobs** carry `runs-on`, `services`, setup steps, `permissions` and pinned actions (by commit) as
  configuration, and keep the names the configuration gives. A generated first job lists the projects
  a change touches; heavy jobs declare the projects they need and are skipped at job level when none
  changed. The workflow's `branches` are globs passed through as written (default `["main"]`); a job
  and each of its steps may set `working-directory`; steps run `before` or `after` the gate commands,
  and an `after` step may carry an `if:` (for example to upload reports on failure); a job that runs a
  history rule checks out the full history, and any job may set `fetch-depth`. Every generated
  checkout sets `persist-credentials: false`, with no setting to keep them: the jobs never push, and
  they read private plugin releases through `GATE_GITHUB_TOKEN`, not through git's credentials. The
  workflow has one `concurrency` group per workflow file and ref (`<workflow>@${{ github.ref }}`,
  the file's path, so a workflow of another name never shares it), with `cancel-in-progress` true
  for pull requests only, so a superseded pull-request run stops and a push to main never cancels a
  running main check; no setting changes it.

**Findings**
- **Mandatory decisions on exceptions.** `[exception-policy] require-decision = true` makes a decision
  mandatory on every exception; the core refuses an exception without one when it loads the
  configuration. Whether a decision names a real record is the `ledger/exception-decisions` rule's
  question. Option data that exempts something is written as an exception, not as data.
- **Baselines.** `gate baseline write` stores the fingerprints of today's findings in
  `gate/baseline.json`; findings in it are not reported, entries that match nothing are, so a baseline
  only shrinks. `gate baseline prune` removes those entries (an aspect set to `off` while its rule
  runs makes its entries stale too), re-keys an entry written before its plugin keyed the finding,
  and never adds one, so a fix shrinks the baseline without accepting a new finding; `write` is for
  the first baseline.

**Cache and speed**
- Compiled modules are keyed by (plugin sha256, wasmtime version, target triple); wasmtime bounds
  them itself. Downloads are pinned: `gate cache prune` removes one only when no lock names it.
  Results and declarations (each stored by plugin) and git answers are derived, recomputed on a
  miss: `prune` removes those of an unpinned plugin and the stored contents no answer names, and
  `--older-than <days> | --max-size <size>` trims them by last use, which a run sets each time it
  uses one.
- A dependency change re-runs the dependent project's steps; its rule results come from the cache when
  its own inputs did not change; the report says "cached" per call.
- The speed bound applies to two runs: the full run, and `check --staged` with one changed file and a
  warm cache.

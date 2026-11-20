<!--
The lead writes a version's section before tagging it, newest first, in the format of the other
ElsiKora repositories; the release takes that section as its notes and fails without it:
a minor or major release is an `#` heading, a patch an `##` heading, each with `### Features` and
`### Bug Fixes` entries of the form `- **scope:** subject ([sha](commit link))`.
Entries link their commit from the second release on; the first release's section, 0.1.0, links
none, because the public history starts at a fresh root at 0.1.0.
Each shared plugin keeps its own changelog next to its code.
-->

# 0.3.0 (unreleased)

### Features

- **plugin-node:** a new shared plugin, `node`, checks that every Node project of a kind uses one
  toolchain: pnpm pinned exactly, one Node version, exact dependency versions, one Prettier
  configuration, one ESLint configuration on the shared package, the standard manifest fields and
  script names, and one strict TypeScript base. Its eight rules are listed in
  [docs/guide/plugins.md](docs/guide/plugins.md#node).

## Unreleased

### Features

- **host:** the workflow `gate sync` writes from `[ci.github]` has a `concurrency` group per
  workflow file and ref: a newer pull-request run cancels the older one still in progress, and a
  push to main never cancels a running main check. A workflow written by 0.2.0 is drift; run
  `gate sync`.

### Bug Fixes

- **host:** a download that stops sending fails within 30 s with an error that names the URL and says
  to try again; before, `gate lock`, `gate sync` and a plugin or tool fetch waited up to 15 minutes
  per request without a word. Each request has 30 s to resolve and connect and 30 s for each read,
  and a download's redirect hops share one 15-minute limit.
- **launcher:** `bin/gate` bounds its core download the same way (curl's `--connect-timeout`,
  `--speed-time` and `--max-time` from one 15-minute deadline; wget's `--timeout`, one try), where
  curl had no read bound and wget retried 20 times. Run `gate init` and commit `bin/gate` to take
  the new launcher; `gate doctor` says when it differs.

### Documentation

- **docs:** upgrading a baseline written by 0.1.0. A finding of a project below the repository root
  that sits in or names one of its option files changed path and message in 0.2.0, so
  `gate baseline prune` cannot re-key its entry and removes it. The baseline section of the
  configuration guide gives the upgrade: prune, check, pair each reported finding with an entry prune
  removed, and when every finding pairs, `gate baseline write` once.

# [0.2.0](https://github.com/ElsiKora/Gate/compare/v0.1.0...v0.2.0) (2026-11-19)

The plugin interface is `gate:plugin@0.2.0`, and every plugin of this repository is released at
0.2.0 with the core: pin `ElsiKora/Gate:<plugin>@0.2.0` with a 0.2.0 core. A 0.2.0 core loads only
plugins built for 0.2.0; one built for 0.1.0 fails to load with an error that names the interface
and the release to pin.

### Features

- **plugin interface:** the git interface answers the commit message being written (`message`, from
  `gate check --commit-message`) and whether a git hook runs the check (`hook`, from `--hook`, which
  the generated hooks pass). ([ccad7cd](https://github.com/ElsiKora/Gate/commit/ccad7cd14c8a3bf624c600c889ae0f4ab6b4c086))
- **host:** a plugin option an enabled rule declares it reads (`Rule::NEEDS` in the SDK) and that is
  missing from `[plugins.<name>] options` is a configuration error at load, exit 2, naming the rule. ([58800b7](https://github.com/ElsiKora/Gate/commit/58800b73ed09b06aebb3fc147aaa0d81e9d0befb))
- **host:** a preset may set the levels of single aspects (`levels`); the project's `[rules]` level
  of the rule, or of the aspect, wins over them. ([a2905b4](https://github.com/ElsiKora/Gate/commit/a2905b456554a86f5c19bc33fb9b525efbc6d36f))
- **host:** a check with `--commit-message` says so in its summary: `gate check (tree, commit
  message)`. ([b01fbf7](https://github.com/ElsiKora/Gate/commit/b01fbf794489f5cad05ca696c8739838f2f8dcea))
- **plugin interface:** a finding may carry a `key`, the plugin's short, stable identity of what it
  is about (`.key(..)` in the SDK). With a key, the fingerprint that baselines and SARIF match is the
  rule, aspect, path and key, so a reworded message keeps its baseline entry; without one it is the
  rule, aspect, path and message, as before. ([b68d372](https://github.com/ElsiKora/Gate/commit/b68d3721cf7c17c8ae83a80d60ed11bfdfe3aa43))
- **gate:** `gate baseline prune` re-keys the baseline entries written before their plugin keyed
  the finding, and removes the entries that match no finding although their rule ran, an aspect
  set to `off` included; it prints each one and never adds an entry, so a new finding stays
  reported. `gate baseline write` is for the first baseline. ([37e7dc4](https://github.com/ElsiKora/Gate/commit/37e7dc448b8de2683b8d2c00fc5632bb040b881d))
- **plugin interface:** `check` and `generate` fail with a `plugin-error`, `configuration` or
  `environment`, in place of a string. A `configuration` error ends the run as a configuration
  error (exit 2, `configuration error`), as a wrong `gate.toml` key does; an `environment` error
  stays an environment error. ([883012d](https://github.com/ElsiKora/Gate/commit/883012d3811891eb9f818213dfe52814bd9d8ee6))
- **sdk:** `Error::config` is a configuration error beside `Error::new`, an environment failure.
  Options that do not match a rule's or generator's options type, and `[plugins]` options or
  `[languages]` settings that do not match, are configuration errors. ([883012d](https://github.com/ElsiKora/Gate/commit/883012d3811891eb9f818213dfe52814bd9d8ee6))
- **sdk:** a plugin may share work among the rules of one `check` call through a state of its own,
  `CallState`, declared with `call:` in `plugin!` and reached by a rule through `Run::with_call`;
  the call builds it before its first rule and drops it after its last. `Rule`, `Run`,
  `RuleEntry` and `PluginSpec` take the state as a type parameter that defaults to `()`, so a
  plugin without one compiles unchanged. `Call` is what one call is, alike for every rule of it. ([fc44cf1](https://github.com/ElsiKora/Gate/commit/fc44cf148a31ef107e231e515553fea6cb2e59cd))
- **typescript:** `Source::bytes_for_parse` reads a file the view is about to parse, so a source
  that keeps what it reads may drop its copy of a UTF-8 file, whose parse keeps the text; it
  defaults to `Source::bytes`. `TypeScript::parsed` answers a file the view already parsed without
  parsing it, and `TypeScript::resolve` answers each import once per view. ([18ec73c](https://github.com/ElsiKora/Gate/commit/18ec73ce696ad9bed097c6da0235591340acb887))

### Bug Fixes

- **gate:** `gate init` suggests only rules that exist, with the options they need, so uncommenting
  every suggestion checks clean; it suggested `git/commit-subject`, which does not exist. ([b6a2c93](https://github.com/ElsiKora/Gate/commit/b6a2c93266114f919f58620fcdd23307fd151133))
- **runtime (security):** wasmtime and wasmtime-wasi 50.0.2, for RUSTSEC-2026-0566 (unbounded
  host memory growth through empty writes to `AsyncWriteStream`), RUSTSEC-2026-0567 and
  RUSTSEC-2026-0570 (aborts in builds without the `threads` or `gc` feature), RUSTSEC-2026-0568 (a
  stale GC heap base pointer lets a guest abort the host), RUSTSEC-2026-0569 (a panic in
  `Caller::get_module_export`), RUSTSEC-2026-0571 (unbounded heap use through empty HTTP/2 body
  writes in wasmtime-wasi-http), RUSTSEC-2026-0572 (a panic when a component cancels a stream or
  future copy on the wrong end) and RUSTSEC-2026-0573 (an abort when a GC heap's reservation is not
  a multiple of the page size). ([5912bb1](https://github.com/ElsiKora/Gate/commit/5912bb1368151170453b9e2b89a6e41a8fc4ed90))

# 0.1.0

The first release: the core, the plugin runtime, the SDK and the launcher. The seven plugins are
released on their own; each keeps its changelog next to its code under `plugins/`.

### Features

- **host:** one configuration file per repository, `gate.toml`, with nested `gate.toml` files and
  profiles for projects. Unknown keys, unknown rules, options that fail a schema and exceptions
  without a reason are configuration errors, exit 2, that name the file, the line and the key,
  raised before anything runs. A plugin's own `[plugins.<name>] options` are checked against the
  schema the plugin declares, and so is every `[generate]` key, when the plugins load.
- **host:** projects from pnpm workspaces, the `workspaces` of `package.json`, explicit globs or
  nested `gate.toml` files; a path belongs to the project with the longest root that contains it.
  `gate list` shows the projects and `gate list --origin` where each setting came from.
- **host:** `gate check` on the whole tree, the staged files (`--staged`), the changes since a base
  (`--changed --base <rev>`), a pushed range (`--push <base>..<tip>`) or one commit (`--rev`, whose
  base is its parent). The report says which projects ran and which were skipped and why.
- **host:** reports as text, JSON, SARIF or GitHub annotations, with findings in a stable order and
  a stable fingerprint; levels `error`, `warning`, `note` and `off` per rule or aspect, and the
  repository's own rule id printed with each finding. `--list-changed-projects` prints only the
  projects a change selects, also as GitHub step outputs.
- **host:** baselines (`gate baseline write`) and exceptions that only shrink: an entry or an
  exception that matches nothing is reported.
- **host:** an exact result cache keyed by the plugin's sha256, the rules, the options and the blob
  ids of every file a plugin read, shared by every worktree and kept outside the repository;
  `gate cache prune` lists the entries no `gate.lock` pins and removes them with `--yes`.
- **host:** `gate sync` writes the git hooks, the CI workflow and the files plugins generate, and
  `gate sync --check` reports drift. A generator that fails is a `gate/sync#<generator>` finding.
  The CI workflow can end in one aggregate job to require, and it reads private plugin releases with
  a token.
- **host:** `gate run` runs the repository's own commands as named steps and suites, per project or
  at the root, on the staged or changed files if asked. A step started from a hook sees the hook's
  arguments and input. External tools are pinned by version and sha256, and only the tools of the
  steps that run are fetched.
- **host:** `gate init`, `gate doctor`, `gate explain`, `gate schema` and `gate lock`; time budgets
  with `--budget`, `--budget-fails` and `--timings`.
- **host:** plugins pinned like the core: one source form, `<owner>/<repo>:<plugin>@<version>`,
  resolved to a URL and a sha256 in `gate.lock`; a checksum that does not match fails closed.
  `--plugin <name>=<file.wasm>` tries a local build for one run, outside the cache.
- **runtime:** plugins are WebAssembly components that read only what the core hands them. Rules
  that need git history declare it and get a read-only view of the commits, and history rules over a
  long history cost only the new commits after the first run. Every plugin call has a fuel limit and
  a deadline that count only the plugin's own work.
- **sdk:** the Rust SDK for rules, generators and languages, with the `plugin!` declaration, the
  options schema of the plugin and of each rule, and `gate-test` for fixture trees.
- **release:** `bin/gate`, a short launcher committed to the repository, which installs the core
  pinned in `gate.lock`, from a public or a private release. Components are reproducible: their
  sha256 depends only on the source, `Cargo.lock` and the pinned toolchain. A release's notes are
  its section of the changelog.

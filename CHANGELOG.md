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

### Breaking Changes

- **lock:** `gate lock --set <name>=<url> --file <path>` replaces `--sha256 <hash>`, with no alias.
  The pin takes the sha256 of the file and the file enters the blob cache, so `gate check` runs
  exactly the build a release job is about to publish, before its asset exists. A release job that
  passed `--sha256` passes the built file with `--file` instead.
- **sdk:** the plugin interface is `gate:plugin@0.3.0`: a generated file carries `seed`, and
  `generate` receives the plugin's `[plugins.<name>] options` as `plugin-options`. A plugin built
  for 0.2.0 does not load; pin the plugins released with this core, or rebuild with `gate-sdk`
  0.3.0.
- **plugin-agents:** `agents/no-claude-md` is a repository-scope rule, set in the root `gate.toml`'s
  `[rules]` only; it reports every `CLAUDE.md` in the repository, and a project file or profile
  that sets it is a configuration error. To migrate: in a monorepo whose root is not a project,
  declare `agents` in the root `[plugins]` and run `gate lock`; set `"agents/no-claude-md"` in the
  root `[rules]` only; remove it from every project's `gate.toml` and every profile; then delete
  each `CLAUDE.md`, or move what it holds into `AGENTS.md`.
- **plugin-typescript:** `nestjs/string-enums` is now `typescript/string-enums`, and no alias keeps the
  old id: a `[rules]` entry or a profile that names `nestjs/string-enums` is a configuration error
  (exit 2, `plugin nestjs has no rule string-enums`). Rename the entry and every profile that sets
  it to `typescript/string-enums` and declare the `typescript` plugin; the option
  `shim-enum-names` is gone from it. The rule no longer reads `[plugins.nestjs] options`
  `layout.test-root`: a project whose tests are not in `test` sets the rule's own option,
  `"typescript/string-enums" = { level = "error", options = { test-roots = ["<test-root>"] } }`.
- **plugin-nestjs:** the aspect `error-string-shim` of `nestjs/string-enums` is now the rule
  `nestjs/error-string-shim`, its option `enum-names` (was `shim-enum-names`). Add the rule to
  `[rules]` and to every profile that had `nestjs/string-enums` on.
- **plugin-nestjs:** `layout.cdk-root` and `layout.cdk-code-folders` are removed and the rules no
  longer read a CDK app; list the app as a project of its own with the `cdk` plugin. The
  `asset-storage` aspect `cloudfront-cdk-only` is `cloudfront-infrastructure-only`; rename it in
  `levels` and exceptions.
- **migration:** an `[[exception]]` entry that names `nestjs/string-enums`, or an aspect of it now in
  `nestjs/error-string-shim`, is a configuration error like a `[rules]` entry (exit 2,
  `[[exception]] rule "nestjs/string-enums": plugin nestjs has no rule string-enums`); rename it
  with the rule. A baseline entry's fingerprint holds the rule id, so an entry of `nestjs/string-enums`
  matches no finding of the new ids and, as its rule no longer runs, `gate baseline prune` keeps
  it: remove those entries from `gate/baseline.json` by hand and, in the commit that renames the
  rule and before any other change, run `bin/gate baseline write` once to accept the same findings
  under `typescript/string-enums`. Entries of the renamed
  `asset-storage` aspect are stale and `gate baseline prune` drops them; accept the findings again
  the same way.
- **plugin-nestjs:** `nestjs/raw-config-access` reports under aspects. Its finding of 0.2 is now
  `boundary`; a file outside the boundary that reads `process.env` is `process-env`; and the keys the
  raw configuration getters and `process.env` read are compared with the project's `.env.example`
  (`undeclared`, `unread`, `dynamic`, `example-missing`, `example-syntax`; options `example-file`,
  `getters`, `external-keys`). A baseline entry's fingerprint holds the aspect, so after the upgrade
  check that `gate check` reports the same files under `boundary`, then run `gate baseline write`
  once.
- **plugin interface:** `gate:plugin@0.3.0` adds `tree.size`, the size of a tracked file without its
  contents. A rule that asks only sizes is answered from the result cache (format 4) until a size
  changes. Plugins built for 0.2.0 do not load; take their 0.3.0 releases.
- **host:** each job of `[ci.github]` needs `timeout-minutes`: a job without it is a configuration
  error (exit 2) that names the job, where before the workflow gave the job GitHub's six hours. To
  upgrade, add `timeout-minutes = <minutes>` to every `[ci.github.jobs.<id>]`, a little above the
  job's usual time, run `gate sync` and commit the workflow.
- **host:** the workflow `gate sync` writes meets the `ci/recommended` standard: its top-level
  `permissions` is `{}` (every job already set its own), the checkout step is named
  `Check out the repository`, the `changes` job has a 15-minute timeout, and the aggregate job is
  named by its id, so its check name is unchanged, with a 5-minute timeout. A workflow written by
  0.2.x is drift; run `gate sync`.
- **ci:** `ci/actions-pinned` reports a commit pin whose comment is not the full release, as the
  aspect `version-comment` at the rule's level: a pin with `# v4` that passed under ci 0.2.0 fails
  at `error` under 0.3.0. Write the release the commit is (`# v4.2.2`), or set
  `"ci/actions-pinned" = { levels = { version-comment = "warning" } }` while the pins are updated.

### Features

- **plugin-node:** a new shared plugin, `node`, checks that every Node project of a kind uses one
  toolchain: pnpm pinned exactly, one Node version, exact dependency versions, one Prettier
  configuration, one ESLint configuration on the shared package, the standard manifest fields and
  script names, and one strict TypeScript base. Its nine rules are listed in
  [docs/guide/plugins.md](docs/guide/plugins.md#node).
- **plugins:** a pinned plugin whose blob is not in the cache and whose asset is not found (HTTP
  404 from a URL outside GitHub, or a GitHub release without that asset) says what to do: publish
  the asset, or admit the build with `gate lock --set <name>=<url> --file <path>`. A refused or
  missing credential keeps its own message, since the asset may be private.
- **host:** seeds. `gate sync` writes a generated file marked `seed` only when nothing exists at its
  path, never overwrites it, and `gate sync --check` never reports it; a seed under a glob any
  generator owns is refused, since `gate sync` would remove it as stale. A generator reads the plugin's options (`Generate::plugin_options`) and
  writes a seed with `Generate::seed`.
- **plugin-docs:** a new plugin, `docs`: one documentation layout per kind of project (`service`,
  `frontend`, `infrastructure`, `library`, `monorepo-root`), the rule `docs/layout` that checks it,
  and the generator `docs/layout` that seeds a project's required index pages.
- **host:** a required `[plugins.<name>] options` key that is missing names where to set it and,
  when its schema lists them, the values it takes.
- **plugin-cdk:** the `cdk` plugin, one standard for every AWS CDK app written in TypeScript,
  checked from the tracked files without running the app: fifteen rules (layout, file names,
  declarations, barrels, `cdk.json` and `package.json`, accounts and regions, stack names, construct
  ids, physical names, properties, tags, removal policies, cdk-nag 3 registered on the App, ECS
  container health checks and unit tests) and the presets `recommended` and `adoption`. Its rules
  are on [plugins/cdk/rules](plugins/cdk/rules/README.md).
- **plugin-docker:** a new plugin, `docker` 0.3.0, with `docker/dockerfile` and `docker/compose`, and
  the presets `docker/recommended` and `docker/adoption`.
- **plugin-repository:** a new plugin, `repository` 0.3.0, with `repository/tracked-secrets` and
  `repository/large-files`, and the presets `repository/recommended` and `repository/adoption`.
- **sdk:** `gate_sdk::yaml`, behind the feature `yaml`: a YAML reader that keeps each node's line,
  for rules that report at a line of a YAML file.
- **plugin-frontend:** a new plugin, `frontend`: one standard for React apps and libraries, with the
  packs `react-vite`, `nextjs` and `react-library`. Its 20 rules hold the tree of Feature-Sliced
  layers, names, import direction, paths and public APIs, components and hooks, stores, the network
  and environment boundaries, literal text and messages, assets, styles, route files, the Next.js
  client boundary, the alias and route folders in the toolchain files, and one tool per job; the
  presets are `frontend/recommended` and `frontend/adoption`. Its rules are listed in
  [docs/guide/plugins.md](docs/guide/plugins.md#frontend).
- **plugin-git:** `git/commit-message` takes `prohibited-body-lines`, regular expressions of the
  body lines a message may not carry, so a note such as `Generated with ...`, which is no trailer,
  can be refused in the `commit-msg` hook and in the pushed commits. `body-full-stop` judges the
  body above its trailer block, so a person's `Co-authored-by` no longer fails it.
- **host:** `[ci.github]` `checkout` and a step's `uses` take the release after the pin,
  `actions/checkout@<sha> # v4.2.2`, which `gate sync` writes as a comment after the pin, as a
  workflow written by hand carries it; the pin before ` # ` is checked as before. A job takes
  `environment`, the deployment environment whose protection rules guard what it writes.
- **ci:** the plugin holds GitHub Actions workflows to one standard, turned on with
  `extends = ["ci/recommended"]`: job permissions, concurrency, timeouts, versioned runners, release
  comments on action pins, checkout credentials, secrets in steps, environments for jobs that write,
  Node setup and names. See `plugins/ci/CHANGELOG.md`.

### Bug Fixes

- **runtime:** `wasmtime` and `wasmtime-wasi` 50.0.3, for RUSTSEC-2026-0587, a WASI filesystem
  sandbox escape on Windows through a drive-prefixed path component, which 50.0.2 had. The
  released binaries, macOS arm64 (`aarch64-apple-darwin`) and Linux musl
  (`x86_64-unknown-linux-musl`), were not affected: the advisory is Windows-only.

## [0.2.1](https://github.com/ElsiKora/Gate/compare/v0.2.0...v0.2.1) (2026-11-20)

The plugin interface stays `gate:plugin@0.2.0`, so every plugin built for 0.2.0 loads in a 0.2.1
core. The git and nestjs plugins are released at 0.2.1; agents, ci, ledger, markdown and typescript
stay at 0.2.0. To upgrade, set `[gate] version = "0.2.1"`, pin `ElsiKora/Gate:git@0.2.1` and
`ElsiKora/Gate:nestjs@0.2.1` where you use them, run `gate lock`, then `gate sync` and `gate init` as
the entries below say.

### Features

- **host:** the workflow `gate sync` writes from `[ci.github]` has a `concurrency` group per
  workflow file and ref: a newer pull-request run cancels the older one still in progress, and a
  push to main never cancels a running main check. A workflow written by 0.2.0 is drift; run
  `gate sync`. ([cf3add5](https://github.com/ElsiKora/Gate/commit/cf3add57da2824d4d249594617b5023538cfa0b1))
- **gate:** `gate cache prune --older-than <days>` and `--max-size <size>` (`512M`, `2G`) trim the
  results and git answers by their last use, the least recently used first over the size; a
  pinned download is never removed, and when the pinned downloads alone exceed the size, prune
  says so. Without a bound, prune also removes the results of a plugin no lock pins and the stored
  file contents no stored answer names (after a day, as a run stores a content before its answer);
  each listed entry says why it goes. An answer file prune cannot read stops it before anything is
  removed. The result cache moves to `results/<plugin sha256>/` (format 3), and prune removes the
  format-2 entries. ([b10eb90](https://github.com/ElsiKora/Gate/commit/b10eb903da739c6b239257634494bb3470683e8b))

### Bug Fixes

- **host:** a download that stops sending fails within 30 s with an error that names the URL and says
  to try again; before, `gate lock`, `gate sync` and a plugin or tool fetch waited up to 15 minutes
  per request without a word. Each request has 30 s to resolve and connect and 30 s for each read,
  and a download's redirect hops share one 15-minute limit. ([dfb766b](https://github.com/ElsiKora/Gate/commit/dfb766b60b15b7b50c209997e640040eefbafaf8))
- **launcher:** `bin/gate` bounds its core download the same way (curl's `--connect-timeout`,
  `--speed-time` and `--max-time` from one 15-minute deadline; wget's `--timeout`, one try), where
  curl had no read bound and wget retried 20 times. Run `gate init` and commit `bin/gate` to take
  the new launcher; `gate doctor` says when it differs. ([1e96a8e](https://github.com/ElsiKora/Gate/commit/1e96a8e596158fbca0f560bbb57f0624bd7c5dcf))
- **host:** a stored `read-at` answer whose file content neither the cache nor git holds is asked
  of git again, as if it were not stored; where git cannot read it either, the error says to run
  `git fetch` and no longer suggests `gate cache prune`, which did not help. ([b10eb90](https://github.com/ElsiKora/Gate/commit/b10eb903da739c6b239257634494bb3470683e8b))

### Performance

- **runtime:** a plugin's listing of a prefix (`Tree::contains`, `Tree::under`) is a binary search
  of the sorted snapshot instead of a scan of every tracked path: a probe takes the search and the
  paths that start with it. The answers and the recorded read set are unchanged, and plugins
  built for 0.2.0 get it without a rebuild. A prefix that can match the `../` form of a read path
  outside the project still lists by a scan. ([a90b87d](https://github.com/ElsiKora/Gate/commit/a90b87ddfda3a3ded2f45fc13cdebbe772a0eca4))
- **host:** a check compiles only the plugins it runs: those declared in a scope the change
  selection runs that `--rule` allows. What every other declared plugin declares comes from the
  cache, where each plugin's declaration is stored when it loads, so the whole configuration is
  still checked in every run. `gate sync` compiles every declared plugin and stores every
  declaration, so after it no check compiles a plugin it does not run. The declarations are
  derived like the results: `gate cache prune` removes those of a plugin no lock pins, and its
  bounds trim the others by last use. On an export of a consumer repository's NestJS service,
  after `gate sync` with the compiled modules removed, `gate check --rule 'nestjs/*'` compiled 2
  modules instead of 8 and took 2.74 s instead of 7.00 s. ([209c6c2](https://github.com/ElsiKora/Gate/commit/209c6c2188208f7b94707f99ea043a46638e383f))

### Changed

- **host:** a `gate check --commit-message` run checks only the message being written: the
  enabled repository rules that declare `READS_MESSAGE`. It no longer checks the work tree; the
  tree stays with `pre-commit`, `pre-push` and CI. Its summary says `commit message`.
  `--commit-message` with `--staged`, `--changed`, `--push` or `--rev` is a usage error, and a
  `[hooks] commit-msg` entry that combines them is a configuration error. `git/branch-name`, which
  does not read the message, now runs only in `pre-commit`: an empty commit
  (`git commit --allow-empty`) on a protected branch is no longer checked by a hook; CI still
  checks it.
- **sdk:** the plugin interface is `gate:plugin@0.3.0`. A rule declares `READS_MESSAGE` to read
  the commit message, through `Run::message` (`Git::message` is gone); `rule-info` carries it as
  `reads-message`. `Run::git` answers only a rule that declares `READS_HISTORY`, so a rule that
  declares only `READS_MESSAGE` reads no history. A 0.3.0 core loads only plugins
  built for 0.3.0, and every plugin of this repository is released at 0.3.0 with the core.

### Documentation

- **docs:** upgrading a baseline written by 0.1.0. A finding of a project below the repository root
  that sits in or names one of its option files changed path and message in 0.2.0, so
  `gate baseline prune` cannot re-key its entry and removes it. The baseline section of the
  configuration guide gives the upgrade: prune, check, pair each reported finding with an entry prune
  removed, and when every finding pairs, `gate baseline write` once. ([2be17f4](https://github.com/ElsiKora/Gate/commit/2be17f4716a7df7672fc40671cfb00dd9514ee97))

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

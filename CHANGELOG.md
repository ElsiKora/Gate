<!--
The lead writes a version's section before tagging it, newest first, in the format of the other
ElsiKora repositories; the release takes that section as its notes and fails without it:
a minor or major release is an `#` heading, a patch an `##` heading, each with `### Features` and
`### Bug Fixes` entries of the form `- **scope:** subject ([sha](commit link))`.
Entries link their commit from the second release on; the first release's section, 0.1.0, links
none, because the public history starts at a fresh root at 0.1.0.
Each shared plugin keeps its own changelog next to its code.
-->

## [0.3.4](https://github.com/ElsiKora/Gate/compare/v0.3.3...v0.3.4) (2026-11-26)

The plugin interface stays `gate:plugin@0.3.0`: its types are unchanged, so no plugin needs a
rebuild, but the repository call's `check-request.rule-decisions` now lists every exception.
`ledger` is released at 0.3.3; the other twelve plugins keep their versions. core 0.3.4 needs ledger
0.3.3, and ledger 0.3.3 needs core 0.3.4: upgrade both together. On a synthetic root project with a
nested project, ledger 0.3.2 on core 0.3.4 reported each of the 3 exceptions that name no decision
as an unlisted decision, and ledger 0.3.3 on core 0.3.3 judged only the nested file, which has a
decided exception, and missed the completed owner in the root file. The mismatch is harmless when
every exception names a decision, as under `[decision-policy] require-decision`: then both cores
list the same files.

To upgrade, set `[gate] version = "0.3.4"`, move a `ledger` pin to 0.3.3 and run `gate lock`. Then
replace `ledger/root-exception-owners` with `ledger/exception-owners` in the root `gate.toml`, at
the level your projects ran `ledger/exception-owners` at, and remove every project's entry for it:
the merged rule judges every project's exceptions, so a root `off` turns owner checks off for the
whole workspace.

### Features

- **host:** the repository call lists every `[[exception]]` of every `gate.exceptions.toml` in
  `check-request.rule-decisions`, with an empty `decision` when it names none, at its repository path
  and line; rule entries, families and `[[profiles]]` rules are listed only when they name a
  decision, as before. A repository-scope rule about exceptions sees exactly the exceptions files the
  core read. ([71fa332](https://github.com/ElsiKora/Gate/commit/71fa3326b8317668cbafdbc1d66bb0164cbbdf09))

## [0.3.3](https://github.com/ElsiKora/Gate/compare/v0.3.2...v0.3.3) (2026-11-26)

The plugin interface stays `gate:plugin@0.3.0`: its types are unchanged, so no plugin needs a
rebuild, but what the repository call carries in `check-request.rule-decisions` changes. `ledger` is
released at 0.3.2; the other twelve plugins keep their versions. ledger 0.3.2 needs core 0.3.3 or
later, and core 0.3.3 needs ledger 0.3.2: upgrade both together. With core 0.3.2, ledger 0.3.2
checks no decision under a root project, and only the root gate.toml's rule entries under a root
that is not a project. With ledger 0.3.1, core 0.3.3 reports each exception's decision twice.

To upgrade, set `[gate] version = "0.3.3"`, move a `ledger` pin to 0.3.2 and run `gate lock`. Four
changes can need an edit of the configuration:

- `ledger/rule-decisions`, `ledger/root-rule-decisions`, `ledger/exception-decisions` and
  `ledger/root-exception-decisions` are one repository-scope rule, `ledger/config-decisions`: turn
  it on in the root `gate.toml`, whose `[plugins.ledger]` names the decision index by its repository
  path, and remove the four old entries.
- A repository-scope rule that a project turns on (a preset, a family or a `[rules]` entry) while
  the root's configuration does not name it is a configuration error: declare the plugin in the root
  `gate.toml` and turn the rule on or set its level there.
- Under `[decision-policy] require-decision = true`, a level below `error` for a rule or an aspect
  that no preset names needs a `decision`, as a preset-named rule's does.
- A root project's rules no longer report a file that a nested project owns. An exception the root
  wrote for such a finding matches no finding and is reported, and a baseline entry for one goes
  stale; move them to the nested project, or remove them.

### Features

- **host:** the repository call carries every decision the workspace's configuration writes, whether
  or not the root is a project: the rule entries and `[[profiles]]` rules of the root `gate.toml`,
  the rule entries of every project's `gate.toml`, and the `decision` of every `[[exception]]` of
  every `gate.exceptions.toml`, each at its repository path and line. A project's call carries none,
  so a repository-scope rule judges each decision once, a nested project's included.
  `check-request.config-file` is unchanged. ([376a9c9](https://github.com/ElsiKora/Gate/commit/376a9c976b8fbaf8a8438cdb637b03381b61ad6c))

### Bug Fixes

- **host:** a rule or plugin option the schema rejects gets a plain message naming the option path,
  the allowed forms and what was given, instead of the validator's text (oneOf, anyOf, not, const,
  enum, type, required, unknown keys, false schema, contains, if/then/else); a plugin that takes no
  options says "this table takes no keys". ([80db512](https://github.com/ElsiKora/Gate/commit/80db5123a13e56820e67f2cfde45dfededc083ec))
- **host:** under `[decision-policy] require-decision = true`, a rule or an aspect that no preset
  names has the default `error`, the level it runs at when nothing sets one, so lowering it to
  `warning`, `note` or `off` needs a decision; a rule entry under a family has the family's level as
  its default. ([2483e35](https://github.com/ElsiKora/Gate/commit/2483e35b6aa2d769c933d614d8a92e9c9847455e))
- **host:** `gate list` prints every rule `gate check` runs, the repository-scope rules that only a
  preset turns on included, from the rule set the check resolves, each with the row that sets its
  level; `gate explain` prints every preset row of a rule. ([b78e008](https://github.com/ElsiKora/Gate/commit/b78e00819278f99bba83246b6454a7a69ca989e7))
- **host:** a root project's rule reported findings on the files of a nested project, which owns
  them. A project's call still reads a nested project's files to resolve them, but its findings on
  them are dropped, except at a rule's own option file that lies there. ([025cd57](https://github.com/ElsiKora/Gate/commit/025cd579926721e0b94a68c0a616bde2beb0a35e))
- **host:** a repository-scope rule that a project's scope turns on (a preset, a family or a
  `[rules]` entry) while the root's configuration does not name it was dropped without a message, so
  it never ran. It is now a configuration error (exit 2) that names the rule, the project and the
  entry, and says to declare the plugin in the root `gate.toml` and turn the rule on or set its
  level there. A root `[rules]` entry, family or preset that names the rule decides it at any level,
  `off` included. ([376a9c9](https://github.com/ElsiKora/Gate/commit/376a9c976b8fbaf8a8438cdb637b03381b61ad6c))

## [0.3.2](https://github.com/ElsiKora/Gate/compare/v0.3.1...v0.3.2) (2026-11-23)

The plugin interface stays `gate:plugin@0.3.0`, so every plugin built for 0.3.0 loads in a 0.3.2
core. `cdk`, `frontend`, `ledger`, `nestjs` and `typescript` are released at 0.3.1, faster with the
same findings; the other eight plugins stay at 0.3.0. To upgrade, set `[gate] version = "0.3.2"`,
move the pins of those five plugins you use to 0.3.1 and run `gate lock`; a repository with
`[ci.github]` runs `gate sync` once, since the generated workflow's `cancel-in-progress` line
changes.

### Features

- **host:** `gate check --skip-rule <id>` (repeatable, `<plugin>/*` on the command line) leaves
  rules out of a run after `--rule`, in hook and CI-job `check` entries too (by single rule id).
  The exceptions and baseline entries of a skipped rule are not due. A skip that leaves nothing out
  is a configuration error. The summary and the JSON report (`skipped-rules`) name the rules it
  left out. A plugin whose every rule is skipped is not fetched or loaded, and `gate.lock` is still
  checked against `gate.toml`. There is still no history switch: a hook leaves the history rules
  out by name. ([03ab4b7](https://github.com/ElsiKora/Gate/commit/03ab4b74493f995eb2186d1e79d353cc6261a5f2))
- **host:** the generated CI can keep the Gate cache between runs: with `[ci.github] cache = true`
  (off by default) each job that runs `bin/gate` restores and saves its cache directory with
  `actions/cache`, keyed by the runner, `gate.lock`, the job and the commit; the core and the tools
  are never cached, and plugins are verified by sha256 as on a cold run. `[ci.github]
  aggregate-name` names the aggregate job, which is the check branch protection requires. ([89d58cb](https://github.com/ElsiKora/Gate/commit/89d58cbb7ee46d9117f0f6f167a3d98e0aad6cce))

### Bug Fixes

- **host:** the generated workflow cancels a superseded run of every ref but the default branch, a
  pushed branch without a pull request included; a push to the default branch never cancels a
  running check there. ([89d58cb](https://github.com/ElsiKora/Gate/commit/89d58cbb7ee46d9117f0f6f167a3d98e0aad6cce))

### Performance

- **host, plugins:** a cold `gate check` without the history rules on a large tree (about 11
  thousand tracked files) counts about 4 s (4075 ms at the median of eight runs) against 6973 ms on
  0.3.0, with the same findings: the core asks the file system once per directory whether it is a
  symbolic link; the TypeScript call state keeps a parsed file's bytes, shares a source root's
  import bindings among the call's rules and builds a file's line index when a line is first asked
  for; `regex-automata` is built with its faster engines; `ledger` builds its catalog and reads its
  records once per call; and `nestjs` takes its costliest rules' repeated work out. ([3f4009a](https://github.com/ElsiKora/Gate/commit/3f4009a9cfbd9dd96a227cf7dccb853bf8eb98eb))

### Documentation

- **docs:** a plugin kept in the repository it checks is published before its pin is committed;
  the guides describe the CI cache, clearing a bad cache entry and branch-run cancelling. ([89d58cb](https://github.com/ElsiKora/Gate/commit/89d58cbb7ee46d9117f0f6f167a3d98e0aad6cce))

## [0.3.1](https://github.com/ElsiKora/Gate/compare/v0.3.0...v0.3.1) (2026-11-22)

The plugin interface stays `gate:plugin@0.3.0`, so every plugin built for 0.3.0 loads in a 0.3.1
core, and all 13 plugins stay at 0.3.0. To upgrade, set `[gate] version = "0.3.1"` and run
`gate lock`; a workflow 0.3.0 wrote from `[ci.github]` is not drift, since a configuration without
`needs` generates the same bytes.

### Features

- **host:** a `[ci.github.jobs.<id>]` job runs after other generated jobs: `needs` names them and
  `when = "succeeded"` (every one succeeded) or `when = "completed"` (they finished and the run was
  not cancelled) says how. The workflow gets GitHub `needs:` and an `if:` that starts with
  `!cancelled()` and keeps the job's projects condition; the aggregate job still needs every job.
  An unknown or repeated id, the job itself, `changes`, the aggregate id, a cycle, and `when`
  without `needs` or the reverse are configuration errors. ([4c3f181](https://github.com/ElsiKora/Gate/commit/4c3f181101c11f9f44e98012c6005400a101c07d))

### Bug Fixes

- **deps:** `url` 2.5.9 and `form_urlencoded` 1.2.3, for RUSTSEC-2026-0592: `url` 2.5.8 recursed
  without a limit in `Url::origin()` on nested `blob:` URLs. ([8122c6c](https://github.com/ElsiKora/Gate/commit/8122c6ca34366141ac528776638deb2a3a4f94cb))

### Documentation

- **docs:** a repository that pins a plugin it builds itself adopts a core release that changes
  the plugin interface on one upgrade branch: the core and the shared plugins move to the release,
  its own plugin to a published pre-release build pinned with `gate lock --set` and `--file`, and
  the next release commit moves that pin to the release. The hooks text says a `--commit-message`
  check checks only the message. ([4d07eb1](https://github.com/ElsiKora/Gate/commit/4d07eb152ca58a1b79b257fefb49f00a2ac266e0), [3037e57](https://github.com/ElsiKora/Gate/commit/3037e5731b734c5297647de232b9da4801cd9f26))
- **plugin-git:** the `git/main-history` page points a release that changes the plugin interface
  at the upgrade-branch path, and describes `release-all-pins` only by what it admits. ([b37c468](https://github.com/ElsiKora/Gate/commit/b37c4687d969a0fdfe15028835a00e57af443eb0))

# [0.3.0](https://github.com/ElsiKora/Gate/compare/v0.2.1...v0.3.0) (2026-11-21)

### Breaking Changes

- **host:** an `extends` entry that turns on no rule of any scope it reaches is a configuration
  error (exit 2) that names where it goes; it used to pass and turn on nothing. A member project's
  or a profile's `extends` of a preset of repository-scope rules only, such as
  `extends = ["git/recommended"]`, moves to the root: `[project] extends` when the root is the only
  project, `[workspace] extends` when it is not. A root's `[workspace] extends` of a preset of
  project-scope rules only moves to a `[[profiles]]` entry.

- **lock:** `gate lock --set <name>=<url> --file <path>` replaces `--sha256 <hash>`, with no alias.
  The pin takes the sha256 of the file and the file enters the blob cache, so `gate check` runs
  exactly the build a release job is about to publish, before its asset exists. A release job that
  passed `--sha256` passes the built file with `--file` instead.
- **sdk:** the plugin interface is `gate:plugin@0.3.0`: a generated file carries `seed`, and
  `generate` receives the plugin's `[plugins.<name>] options` as `plugin-options`. A plugin built
  for 0.2.0 does not load; pin each plugin's release built for 0.3.0 (its `CHANGELOG.md` names
  the interface), or rebuild with `gate-sdk` 0.3.0.
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
- **host:** a `gate check --commit-message` run checks only the message being written: the enabled
  repository rules that declare `READS_MESSAGE`. It no longer checks the work tree; the tree stays
  with `pre-commit`, `pre-push` and CI. Its summary says `commit message`. `--commit-message` with
  `--staged`, `--changed`, `--base`, `--push` or `--rev` is a usage error, and a
  `[hooks] commit-msg` entry that combines them is a configuration error. `git/branch-name`, which
  does not read the message, now runs only in `pre-commit`: an empty commit
  (`git commit --allow-empty`) on a protected branch is no longer checked by a hook; CI still checks
  it.
- **sdk:** `gate:plugin@0.3.0` also carries `reads-message` in `rule-info`: a rule declares
  `READS_MESSAGE` to read the commit message, through `Run::message` (`Git::message` is gone).
  `Run::git` answers only a rule that declares `READS_HISTORY`, so a rule that declares only
  `READS_MESSAGE` reads no history.
- **config:** `[exception-policy]` is renamed to `[decision-policy]`, and `require-decision = true` now
  covers every relaxation of a rule, not only exceptions: a `[rules]` entry (a rule or a
  `"<plugin>/*"` family) that sets a level, or a level in `levels`, below its default must name the
  decision that accepted it with `decision = "<record>"`. The default is the preset's level of that
  aspect, else of the rule; a rule no preset names needs no decision at any level. `off` is below
  every level. The check runs when the plugins load and fails with exit 2. `[exception-policy]` is
  now an unknown table: rename it.
- **sdk:** the plugin interface's `check-request` gains `rule-decisions`: each decision a rule entry
  of the scope's own `gate.toml` names, with the entry's key as written and its line (a project's
  file in its project's call, the root file in the root project's call, or in the repository call
  when the root is not a project), and `config-file`, that file's path relative to the scope's root,
  empty in the repository call under a root project. Both are part of the `gate:plugin@0.3.0`
  interface, against which every plugin is rebuilt; a rule reads them with `run.rule_decisions()`
  and `run.config_file()`.
- **sdk:** the plugin interface's `check` export returns a `check-result` instead of a list of
  findings: the findings and `judged`, the targets the call's rules judged in full. It is part of
  the `gate:plugin@0.3.0` interface; a plugin built with `gate-sdk` 0.3.0 returns it without a
  change to its rules.
- **plugin-ledger:** an enabled ledger rule fails when a file it reads is not tracked, instead of
  reporting nothing: each such file is a finding of the new aspect `input` of every rule that reads
  it, at the file's path and keyed by it, naming the option that points there. The manifest itself
  is `ledger/manifest#input`. A project without its ledger now fails; add the files, point the
  options at them, or set `levels = { input = "off" }` with a decision.
- **plugin-ledger:** options without which an enabled rule did nothing are required, and leaving one
  out stops the run with exit 2: `field` of `ledger/closing-notes` and `catalog` of `ledger/reuses`
  have no default, and `ledger/record-status` needs the shared `parity`.
- **plugin-agents:** an enabled agents rule fails when its input is missing: the rules that read the
  rule files (`rule-registry`, `rule-ids`, `rule-file-size`, `rule-enforcement`, `rule-owner`,
  `rule-references`) report an empty `<root>/rules` as `#input`, and `instruction-references`
  reports a missing `entry`. The `agents-block` generator reports an empty `<root>/rules` as
  `gate/sync#agents/agents-block` in `gate sync` and `gate sync --check`, and seeds a missing
  `entry`.
- **check:** a run has two time thresholds, `--warn-after <seconds>` (a `gate/budget` warning) and
  `--fail-after <seconds>` (a `gate/budget` error), replacing `--budget` and `--budget-fails`, which
  are now unknown options. `budget` in `[hooks]` and in a CI job is a table,
  `budget = { warn-after = 3, fail-after = 6 }`, with one key or both; the integer form is refused.
  The generated jobs pass exactly the thresholds they are given and no longer turn a budget into a
  failure on their own, so run `gate sync` after the change.

### Features

- **host:** `gate check` loads a plugin only for a call that misses the result cache; a run whose
  every call is cached reads and loads no plugin. `gate.toml` and `gate.lock` are still checked
  against each other first, and the configuration against what each plugin declares, which the
  cache keeps per plugin sha256, core and plugin interface. `--timings` prints one load line per
  plugin loaded, the plugins not loaded, and how many modules came from the module cache or were
  compiled. `gate doctor` names the plugins of `gate.lock` not compiled under this core yet, which
  `gate sync` compiles.
- **host:** `[workspace] extends` turns on presets for the repository-scope rules of a root that is
  not a project, such as `git/recommended` for `git/branch-name` and `git/commit-message` in a
  monorepo; the root `[rules]` overrides them, and a preset's project-scope rules are skipped there
  without a message. A root that is a project keeps `[project] extends`, and `[workspace] extends`
  there is a configuration error.
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
- **config:** a rule entry's table form takes `decision`, the id of the decision record that accepted
  its levels, and a family wildcard takes `decision` beside its level.
- **plugin-ledger:** `ledger/rule-decisions` reports a decision a rule entry names that the decision
  index does not list as accepted, as `ledger/exception-decisions` does for exceptions. It reads the
  decisions the core hands it, never `gate.toml`.
- **sdk:** `CallState::report` lets a plugin's call state report what it found while a rule used
  it, into that rule's report, after each `Run::with_call` that answers without an error; the
  default reports nothing.
- **plugin-ledger:** the generator `ledger/layout` seeds an empty ledger at the project's paths: the
  manifest (the five shared files under their common root, no identifier family, no forbidden
  reference) and the five shared files, each with a title, the decision index with the block of
  `ledger/decision-counts` and its table header; with a tracked manifest, also each canonical file
  it lists. A tracked file is never seeded. The seeded ledger is structurally complete: the rules
  report only the files the project alone can write (`#input`) and its first implementation and
  production blocker records (`ledger/records`, keyed `#none`).
- **plugin-agents:** `agents/agents-block` seeds `AGENTS.md` when it is not tracked: a title, one
  line, and the managed block filled from the rule files; with nothing tracked under its `root`, it
  also seeds the reading map `<root>/context-map.md`. With the docs, ledger and agents
  generators, one `gate sync` lays down a project's whole documentation standard, and `gate check`
  then asks only for content: each index's `#unfilled`, the rule files' `#input`, the ledger files
  only the project can write, and its first records.
- **plugin-ledger:** at a root that is not a project, the repository-scope rules
  `ledger/root-rule-decisions`, `ledger/root-exception-decisions` and `ledger/root-exception-owners`
  hold the root `gate.toml` and `gate.exceptions.toml` to the decisions and owners they name, with
  the judgement of their project counterparts. A root could relax a rule or accept a finding by
  naming a decision that was unlisted or only proposed, and every check passed.
- **lock:** `gate lock --set` and `--file` may be repeated to pin several builds in one run, the
  first `--file` for the first `--set`; every pin is checked before any build enters the cache or
  `gate.lock` is written.
- **gate:** `gate list` and `gate explain` show the rules a preset turns on, each marked
  `(preset <plugin>/<preset>)` with its `extends` entry as its origin, read from what each pinned
  plugin declares, from the cache only; both take `--plugin <name>=<file.wasm>`. When the
  declarations cannot be read, for example before the first `gate lock` or `gate sync`, the output
  says why.

- **plugins:** every shared plugin ships two presets: `<plugin>/recommended` turns on its rules at
  `error`, and `<plugin>/adoption` the same rules at `warning`, for a repository on its way to the
  standard. `git` and `ci` gain `<plugin>/adoption`; `agents`, `ledger`, `markdown`, `nestjs` and
  `typescript` gain both; `cdk/adoption` turns on every `cdk` rule at `warning`, as the others do.
  A preset leaves out a rule that needs something only the project can give: an option without a
  default or whose default turns the rule off, a plugin option such as `activation`, the
  project's own inventory, or a subsystem a service may not have
  (`nestjs/advisory-lock-manifest`, `nestjs/asset-storage`). The plugins guide names each such
  rule and why.

### Bug Fixes

- **plugins:** a project tsconfig that extends a base above the project root, such as
  `"extends": "../../tsconfig.base.json"` in a monorepo, no longer stops every `nestjs`, `cdk`,
  `frontend` and `typescript` rule that resolves imports with an environment error (exit 2). The
  TypeScript language library reads each base of the `extends` chain through the plugin's tree, so
  a base outside the project is read when it is tracked and granted with `reads`, and an
  unreadable base (not granted, absolute, empty, or a `#` entry whose target cannot be read) is
  left out of the chain and is one finding of the call with what to do.
- **lock:** a release `gate.toml` asks for that does not exist is a configuration error that says
  so and names the repository's releases, when the repository itself answers; it used to blame the
  credential, also for a public repository. `gate lock` names every missing release in one run
  and writes nothing.
- **host:** an `extends` entry that names a preset its plugin does not have lists the plugin's
  presets; it used to send the reader to `gate list`, which did not show them.
- **host:** an option value outside an enumeration names the allowed values (`/pack: "next" is not
  one of "react-vite", "nextjs" or "react-library"`), instead of the schema validator's
  `anyOf` text.
- **check:** the `gate/budget` help sends the reader to the slow step that `--timings` shows,
  instead of a slow plugin.
- **host:** compiling plugins no longer counts toward `--warn-after` and `--fail-after`: the first
  run after a clone, a core upgrade or a plugin bump failed the hook budget while it compiled every
  plugin. A run that compiled says how long it took without the compile, and how long compiling
  took.
- **host:** a `[rules]` entry that sets only a level for a rule a preset turns on keeps the options
  the preset gives it: `extends = ["git/recommended"]` with `"git/branch-name" = "warning"` loads
  and runs with the preset's branch template. It used to be a configuration error (exit 2,
  `options: {} is not valid`), because the check read the entry's missing options as `{}` while the
  run used the preset's. This held in a project's file, a profile and the root `[rules]`. Options a
  preset gives that break the rule's schema are reported as the plugin's error, naming the preset
  and its `extends` entry.
- **host:** an exception is reported as matching no finding (`gate/exceptions`) only where its rule
  judged what it covers. An exception keyed by a commit id, as `git/commit-message`'s help advises
  for a pushed commit others already have, failed every pre-commit, commit-msg and later push run,
  in which the rule ran without judging that commit; an exception for one branch name failed every
  run on another branch. A rule whose subjects change from run to run now says what it judged
  (`Run::judges_only`, and `judged` in the plugin interface's new `check-result`), and re-judges
  the targets of its own exceptions it can judge alone in every run (`Run::rejudges`), so a truly
  stale exception is still reported: `git/commit-message` reads each excepted commit again, alone,
  and `git/branch-name` judges each excepted name. A re-judged target keeps only the findings an
  exception accepts, so a failure the exceptions do not cover is reported by the run that judges
  the target in full, not by every hook. A rule that never says keeps today's behaviour: all its
  exceptions are due whenever it ran. Baseline entries follow the same rule by their key, and a
  target only re-judged does not make them due.
- **runtime:** `wasmtime` and `wasmtime-wasi` 50.0.3, for RUSTSEC-2026-0587, a WASI filesystem
  sandbox escape on Windows through a drive-prefixed path component, which 50.0.2 had. The
  released binaries, macOS arm64 (`aarch64-apple-darwin`) and Linux musl
  (`x86_64-unknown-linux-musl`), were not affected: the advisory is Windows-only.
- **lock:** two runs that write `gate.lock` at once in one worktree (`gate lock`, `gate lock --set`,
  `gate init`) keep both changes: the second waits for the first, says so, and reads `gate.lock`
  after the first wrote it. Before, the run that wrote last removed the other's pin, and both
  reported success. The wait is an advisory lock on `gate.lock.guard` in the worktree's git
  directory, released when a run ends or dies; where it cannot be taken, the error names the path
  and what to do.
- **host:** `gate.lock`, the files `gate sync` writes, and `gate init`'s `gate.toml` and `bin/gate`
  are written to a temporary file, synced to disk and renamed over the old file, and on Unix the
  directory is synced after the rename; a failed write leaves the old file and no temporary file.
  Before, a crash could leave an empty `gate.lock`, and `gate sync` and `gate init` wrote in place,
  so a `bin/gate` that ran meanwhile could read half a launcher.
- **baseline:** `gate baseline write` and `gate baseline prune` replace `gate/baseline.json` whole,
  through the same synced temporary file and rename, so a `gate check` never reads part of it. The
  two take turns on an advisory lock on `gate.baseline.guard` in the worktree's git directory. A
  prune reads the baseline again once it holds the lock, so a baseline another run wrote during its
  check is what it prunes. Before, prune wrote over it from the baseline it had read earlier: an
  entry the other run added was lost, or one it removed came back. An entry the other run wrote is
  kept as it is, and the findings it accepts are its own, so prune never re-keys an older entry to
  one of them and no finding is accepted twice.
- **host:** `gate doctor` before the first `gate sync` says the hooks are not written yet and asks
  for `gate sync`, instead of saying the hooks in `.githooks` do not run while `.githooks` does not
  exist; `gate sync` says `set core.hooksPath to .githooks (it was not set)` or `(it was <path>)`,
  instead of `(unset)`, which read as the new value.
- **host:** a root's `[workspace] extends` entry whose plugin the root does not declare names both
  places at once: the root's `[plugins]` for the preset's repository-scope rules, or a
  `[[profiles]]` entry with the plugin in its `[profiles.plugins]` for its project-scope rules.
  Before, a preset of project-scope rules took two errors to place.
- **cache:** `gate cache prune --yes` takes the `gate.lock` guard of every worktree of each
  repository it is given, each git directory once and in one fixed order, and holds them from its
  first lock read to its last removal; a listing without `--yes` takes none. Before, a prune that read the
  locks while `gate lock --set --file` had admitted a build but not yet written its pin removed that
  build, and the next `gate check` failed on a pin whose blob was gone and whose asset was not
  published.
- **host:** the summary line of `gate check` counts the findings the baseline holds back, as in
  `0 errors, 0 warnings, 12 findings held back by gate/baseline.json`, and `--format json` has the
  count as `baselined` (SARIF as the run's `properties.baselined`). Before, a run after
  `gate baseline write` printed `0 errors, 0 warnings`, the same as a clean tree.
- **host:** a baseline written over two identical findings passes its own check: identical findings
  are one before the baseline sees them. Before, `gate baseline write` stored one entry for them,
  and the next `gate check` held back one and reported the other.

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

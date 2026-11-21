# Configuration

Gate reads `gate.toml` files: one at the repository root, and one in each project of a monorepo that
needs settings of its own. This page describes every section. `gate schema` prints the JSON Schema of
the file, which editors use for completion through the `#:schema` line.

## 📄 Files

| File | Where | Holds |
| --- | --- | --- |
| `gate.toml` (root) | the repository root | `[gate]`, `[workspace]`, repository-scope plugins and rules, `[[profiles]]`, root steps and suites, `[run]`, hooks, CI, tools |
| `gate.toml` (project) | a project's root | `[project]`, that project's plugins, languages, rules, generators, steps and suites, `[decision-policy]` |
| `gate.exceptions.toml` | beside a `gate.toml` | the `[[exception]]` entries of that file's scope |
| `gate.lock` | the repository root, written by `gate lock` | the resolved URL and sha256 of the core per platform and of every plugin |
| `gate/baseline.json` | the repository root, written by `gate baseline write`, shrunk by `gate baseline prune` | fingerprints of findings accepted for now |

When the root is the only project (no `[workspace]`, or a workspace that lists `"."`), the root file
also holds `[project]` and the project-scope rules.

A project gets its settings, in this order: the root's repository defaults, every profile whose
`projects` match it, then its own `gate.toml`. Each later source overrides the earlier ones. A project
with no file of its own is fully described by profiles, and a project's own file travels with the
project when it moves to another repository.

`gate list --origin` prints every setting of every project with the file and section it came from.

```toml
#:schema https://github.com/ElsiKora/Gate/releases/download/v0.2.1/gate.schema.json
```

## ✅ Validation

Gate validates every file before it runs anything. These are configuration errors, reported with the
file, the key and what is expected, with exit code `2`:

- an unknown section or key;
- a value of the wrong type;
- a rule id that no configured plugin declares;
- a project-scope rule in the root `[rules]` (the message points to `[[profiles]]`);
- options that fail the rule's JSON Schema, including unknown option keys;
- an exception without a `reason`, or without a `decision` when `[decision-policy]` requires one;
- a rule entry that sets a level below its default without a `decision` when `[decision-policy]`
  requires one (checked when the plugins load, since presets come from plugins);
- a `check` entry of `[hooks]` or of a CI job that `gate check` would refuse (an unknown option, a
  value it refuses, `--fail-after` not above `--warn-after`), checked with the same definition;
- a `gate.lock` that does not match the configuration.

## `[gate]`

```toml
[gate]
version = "0.2.1"
```

| Key | Meaning |
| --- | --- |
| `version` | the core this repository runs; its checksums are in `gate.lock` |

Root file only.

## `[workspace]`

```toml
[workspace]
projects = "pnpm"
ignore = ["samples/**"]
extends = ["git/recommended"]
```

| Key | Meaning |
| --- | --- |
| `projects` | where projects come from: `"pnpm"` (the `packages` of `pnpm-workspace.yaml`), `"npm"` (the `workspaces` of the root `package.json`, which a yarn workspace uses too), or a list of globs such as `["apps/*", "packages/*"]` |
| `ignore` | globs of directories that never become projects; a `gate.toml` below them is not read. Rules still see their files: they belong to the enclosing project, or to the repository |
| `extends` | presets for the repository-scope rules, `<plugin>/<preset>`, at a root that is not a project; the root `[rules]` then overrides any of their rules |

Root file only. See [monorepo.md](monorepo.md).

## `[project]`

```toml
[project]
name = "server"
```

| Key | Meaning |
| --- | --- |
| `name` | the project's name in reports and in `gate list`; defaults to the `name` of the project's `package.json`, then to its root path |
| `extends` | presets to turn on, `<plugin>/<preset>`; `[rules]` then overrides any of their rules |

A project file, or the root file when the root is the only project; a profile takes `extends` for a
set of projects. A preset is a named set of rules with levels, levels of single aspects and options
that a plugin may ship, such as `git/recommended`. The project's own configuration wins over a
preset. A rule's level for one aspect comes, first found, from `[rules]` for that aspect, from
`[rules]` for the whole rule, from the rule's family, from the presets for that aspect, and from the
presets for the whole rule; else it is `error`. So a level `[rules]` gives the whole rule applies to
all its aspects, the ones a preset sets too: with `extends = ["git/recommended"]`,
`"git/commit-message" = "note"` reports `header-min-length` as a note, and `"off"` turns every
aspect off. A preset's aspect levels apply only where `[rules]` sets no level for that rule or
aspect.

Where an `extends` entry goes depends on the kind of root:

| Root | Repository-scope rules | Project-scope rules |
| --- | --- | --- |
| the only project (no `[workspace]`, or `projects` lists `"."`) | `[project] extends` of the root file | `[project] extends` of the root file, or of a member project |
| not a project (a monorepo root) | `[workspace] extends` | `[project] extends` of a member project, or a `[[profiles]]` entry |

A preset may hold rules of both scopes; each place takes the rules of the scope it runs and skips
the others without a message, so a monorepo root can extend `node/recommended` for its six
repository-scope rules while a profile extends it for the projects' three. An entry that turns on no
rule of any scope it reaches is a configuration error that names where it goes instead:
`typescript/recommended` in `[workspace] extends` turns on no repository-scope rule, and a member
project's `extends = ["git/recommended"]` turns on no project-scope rule. A preset that names a rule
of the scope at `off` still counts. A `[workspace] extends` entry whose plugin the root does not
declare names both places at once, since which scope its preset holds is known only once the plugin
loads: the root's `[plugins]` for its repository-scope rules, or a `[[profiles]]` entry, with the
plugin in that entry's `[profiles.plugins]`, for its project-scope rules. `[workspace] extends` in a
root that is a project is a configuration error: its presets go in `[project] extends`, which
applies them to the repository's rules too.

## `[plugins]`

```toml
[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.2.0" }
house = { source = "your-org/your-repo:house@1.2.0" }

[plugins.git]
source = "ElsiKora/Gate:git@0.2.1"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "<the output of git rev-parse HEAD>"  # the commit the policy starts after
release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$'
```

The key is the name the rules use (`typescript/...`). The value is always a table:

| Key | Meaning |
| --- | --- |
| `source` | required: `<owner>/<repo>:<plugin>@<version>` |
| `options` | options shared by all of the plugin's rules, such as a source layout; the plugin's pages describe them |
| `options-file` | a JSON file with the shared options, relative to this `gate.toml`; not with `options` |
| `reads` | paths outside the project the plugin may read (below) |

A source names the asset `<plugin>.wasm` of the GitHub release tagged
`plugin-<plugin>-v<version>` of that repository. `gate lock` resolves it to a URL and a sha256; a
downloaded plugin whose checksum differs is refused.

Each plugin declares the schema of its options, and Gate checks them when the plugins load, before
anything runs: a missing required option, an unknown one or a value of the wrong type is a
configuration error (exit code `2`) that names the file, the line and the plugin. The options belong to
the plugin, not to its rules, so they are checked wherever the plugin is declared, even when every one
of its rules is off. A plugin without options of its own accepts only an empty table. The `git`
options are listed on [git/main-history](../../plugins/git/rules/main-history.md#options).

A plugin of a private repository is fetched with a token: `$GATE_GITHUB_TOKEN`, else `$GITHUB_TOKEN`,
else `gh auth token`. A missing or insufficient credential fails with exit code `2` and names the asset,
the URL and the variable to set. The plugins of `ElsiKora/Gate` are public and need none.

A plugin may need to read paths outside its project, for example records kept by a sibling project.
Grant them explicitly:

```toml
[plugins]
ledger = { source = "ElsiKora/Gate:ledger@0.2.0", reads = ["docs/records"] }
```

A grant is a file or a directory, written from the repository root whichever `gate.toml` names it, with
`/` separators and no `..`. Any path that is not granted reads as absent. A change below a granted path
selects the project, as a change to its own files does.

A pinned plugin can be replaced for one run, to try a change before it is released, with `--plugin` on
the command line; see [writing-a-plugin.md](writing-a-plugin.md#5-try-it-in-a-repository). There is no
way to do that from a file.

## `[languages]`

A language's settings are written once per project and passed to every plugin that declares the
language.

```toml
[languages.typescript]
tsconfig = "tsconfig.json"
skip-folders = [".git", "node_modules", "/build", "/coverage", "/dist"]
```

| Key | Default | Meaning |
| --- | --- | --- |
| `tsconfig` | `"tsconfig.json"` | The tsconfig whose `paths` the module resolver reads, relative to the project root. |
| `skip-folders` | `[".git", "node_modules", "/build", "/coverage", "/dist"]` | The folders a source walk skips. |

`skip-folders` entries match folders the way `.gitignore` does. An entry without `/` names a folder at
any depth: `node_modules` skips `node_modules/a.ts` and `src/node_modules/a.ts`. An entry with a `/`,
leading or inner, names that folder from the project root: `/build` skips `build/a.ts` but not
`src/build/a.ts`, and `infrastructure/cdk/cdk.out` skips only the files below that folder. A trailing
`/` changes nothing. So with the default, a source folder named `build` below `src` is read. Setting
the key replaces the whole list. A rule page says when one of its walks skips no folder.

The module resolver reads the `extends` chain of `tsconfig` through the plugin's own files, each base
by its path from the project root. A base outside the project, such as `../../tsconfig.base.json`
in a monorepo, is read only when it is tracked and granted to each plugin that resolves imports
(`nestjs`, `cdk`, `frontend`, `typescript`), with `reads` written from the repository root:

```toml
[plugins]
nestjs = { source = "ElsiKora/Gate:nestjs@0.3.0", reads = ["tsconfig.base.json"] }
```

A base the plugin cannot read is reported once per run of the plugin, as a finding at the tsconfig
that names it, and resolution goes on without the `paths` and `baseUrl` the base sets. So are a base
named by an absolute path, an empty `extends` entry, and a `#` entry whose `package.json` `imports`
target is a file the plugin cannot read. The finding carries the level of the first rule that
resolved an import, which may be a warning, while the base's absence may also cause other findings
of the plugin, such as an import that resolves to nothing. A base from a package that is not
tracked, such as `@tsconfig/node22`, or a `#` entry that maps to one, is skipped without a finding,
as an import of such a package resolves to nothing. That includes a workspace package whose files
are tracked but which the project reaches only through the untracked `node_modules` link the
package manager makes: no grant lets the plugin follow that link, so extend such a base by its
relative path (`../../packages/tsconfig/base.json`) and grant that path instead.

## `[rules]`

```toml
[rules]
"nestjs/import-direction" = { level = "error", norm = "RULE-IMP-001", options = { contracts = [{ source-directory = "src/modules/domain", aspect = "domain-direction", forbidden-owner-paths = ["src/modules/public"], forbidden-packages = ["@nestjs/swagger"] }] } }
"nestjs/field-inventory" = { options-file = "gate/options/field-inventory.json", levels = { stale-entry = "warning" }, norms = { exposure = "RULE-API-004" } }
"ledger/*" = "off"
```

The key is a rule id, `<plugin>/<rule>`, or a family wildcard (`<plugin>/*`); an aspect is never part
of the key. The value is a level string or a table:

| Key | Meaning |
| --- | --- |
| `level` | `error` (fails the gate), `warning` (reported), `note` (informs) or `off`, for every aspect `levels` does not name; the level is always yours, never the plugin's |
| `norm` | your own rule id, printed with every finding of this rule and by `gate explain` |
| `levels` | levels of single aspects: `{ <aspect> = "warning" }` |
| `norms` | norms of single aspects: `{ <aspect> = "RULE-X-001" }` |
| `options` | the rule's options, validated against its schema |
| `options-file` | a JSON file with the options, relative to this `gate.toml`, for large data; validated against the same schema; not with `options` |
| `decision` | the id of the decision record that accepted this entry's levels; required for a level below its default when [`[decision-policy]`](#decision-policy) says so |

A family wildcard takes only `level` and `decision`; norms, aspect levels and options are set on each
rule. A rule's page lists its options and its aspects.

The root `[rules]` holds repository-scope rules only (for example `git/no-ci-skip`). Project-scope
rules go in a project's own file or in a profile. `gate list` prints every rule that is on, every rule
or family that is off, and the file that turned it off.

## `[[profiles]]`

```toml
[[profiles]]
projects = ["apps/*"]

[profiles.plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.2.0" }
nestjs = { source = "ElsiKora/Gate:nestjs@0.2.1" }

[profiles.rules]
"typescript/parse" = "error"
"nestjs/barrel-exports" = "warning"

[[profiles]]
projects = ["packages/*"]

[profiles.plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.2.0" }
```

Root file only. A profile applies project settings to every project whose root matches one of its
`projects` globs (`!` excludes); a profile that matches no project is a configuration error. It takes
the keys of a project file: `extends`, `plugins`, `languages`, `rules`, `steps`, `suites`, `generate`. Profiles apply in the order they are written, after the root's defaults and before the
project's own file. See [monorepo.md](monorepo.md#-profiles).

## Exceptions

Exceptions live in `gate.exceptions.toml`, beside the `gate.toml` whose scope they belong to (a
project's root, or the repository root), as `[[exception]]` entries. Commit the file: Gate reads
tracked files only.

```toml
# apps/server/gate.exceptions.toml
[[exception]]
rule = "nestjs/typeorm-relation#relation-nullability"
target = "src/modules/user/user.entity.ts:UserEntity.parent"
reason = "historic column, removed by the next migration"
decision = "D-ORM-014"
```

| Key | Meaning |
| --- | --- |
| `rule` | the rule id, with an aspect if the exception is for one aspect |
| `target` | what the exception covers, in the rule's target grammar as its page describes (for example `<path>:<Class>.<member>`), or the path of a finding that has no target; relative to the directory of the file |
| `reason` | required: why the finding stays |
| `decision` | the id of the decision record that accepted it; required when [`[decision-policy]`](#decision-policy) says so |
| `owner` | the record that owns the exception; a plugin may check that it is open |

`gate schema --exceptions` prints the JSON Schema of the file.

A finding that matches an exception is not reported. An exception that matches no finding is reported
as `gate/exceptions`, so exceptions are removed when they are no longer needed. There are no suppression
comments in source files, and data that exempts something is written as an exception, not as a rule
option.

An exception is reported only by a run that judged what it covers:

- A rule that did not run in the exception's scope (it is off, `--rule` leaves it out, or the change
  selection skips the project) judged nothing there.
- Most rules judge their whole scope whenever they run, so each of their exceptions is checked.
- A rule whose subjects change from run to run judges only those subjects, and says so. Examples
  are the commits a push adds and the branch that is checked out. Its rule page lists them. An
  exception for another commit or branch is not checked in that run, so an exception keyed by a
  shared commit's id holds in every hook. Such a rule also re-judges the targets of its own
  exceptions that it can judge alone, in every run: `git/commit-message` reads each excepted commit
  again. So an exception that no longer accepts anything is still reported.
- A re-judged target brings no new finding into the run. The run keeps only the findings on it
  that an exception accepts. Say an exception accepts a commit's `header-max-length`, and a later
  `prohibited-trailers` pattern matches a trailer of that commit. The trailer is reported by the
  run that judges the commit in full, its push, not by every hook on every branch that holds it.

The baseline follows the same rule. An entry with a key is checked by a run that judged that key in
full, and an entry without one by any run of its rule. A target that a run only re-judged does not
make a baseline entry due.

An exception keyed by a commit names the commit's full lower-case id, as the rule's findings do. A
short or upper-case id would never match a finding, so `git/commit-message` and `git/no-ci-skip`
refuse it as a configuration error that gives the command to print the full id.

## Decision policy

A repository can make a decision mandatory on every relaxation of a rule:

```toml
[decision-policy]
require-decision = true
```

A relaxation is an `[[exception]]`, or a `[rules]` entry (a rule or a family) whose `level`, or any
level in `levels`, is below its default. The default is the level a preset the scope extends gives
the rule or aspect: the preset's level of that aspect, else the preset's level of the rule. Below
means a lower level (`error` > `warning` > `note`) or `off`. A rule no preset names has no default:
it runs only because its entry or a family turns it on, which is stricter than not running, so it
needs no decision at any level. A whole-rule `level` replaces the preset's aspect levels too, so it
is compared with each one that `levels` does not name:

```toml
[rules]
# a preset sets the rule to error: a decision names the record that accepted the warning
"nestjs/barrel-exports" = { level = "warning", decision = "D-API-002" }
"ledger/*" = { level = "off", decision = "D-LEDGER-001" }
```

In the root `gate.toml` the policy covers every file, profiles included; a project's own `gate.toml`
can set its own for its exceptions and its rule entries. A decision covers the levels of the entry
that writes it, so a project file that lowers a rule or aspect the root left alone needs its own
decision. The core enforces the policy as a configuration error (exit `2`) that names the key as
written, the level, the default and the preset that sets it, and the form that adds the decision:
for exceptions when it loads the
configuration, for rule entries when it loads the plugins, since presets come from plugins. Without
the policy, or with `require-decision = false`, nothing is checked and `decision` is accepted anyway.

Whether a decision exists is a plugin's check: the `ledger` plugin's `ledger/exception-decisions` rule
reports an exception's decision that its decision index does not list as accepted, and
`ledger/rule-decisions` a rule entry's. The core gives each plugin call the decisions the rule
entries of its scope's own `gate.toml` name, and that file's path, so the plugin never reads
`gate.toml` itself:

| Call | Its own `gate.toml` |
| --- | --- |
| a project's | the project's `gate.toml`; for the root project, the root file |
| the repository's, when the root is not a project | the root `gate.toml` |
| the repository's, under a root project | none: the root file is the root project's |

A monorepo whose root is not a project writes its own decisions and exceptions in the root files:
decisions on the root `[rules]` and `[[profiles]]` entries in the root `gate.toml`, and the
exceptions of repository-scope rules in the root `gate.exceptions.toml`. The repository-scope rules
`ledger/root-rule-decisions`, `ledger/root-exception-decisions` and `ledger/root-exception-owners`
judge them as the project rules judge a project's, with the decision index and records the root's
`[plugins.ledger] options` name, relative to the repository root:

```toml
# gate.toml at a root that is not a project
[workspace]
projects = ["apps/*"]

[plugins.ledger]
source = "ElsiKora/Gate:ledger@0.3.0"
options = { decisions = "apps/docs/decisions/index.md" }

[rules]
"ledger/root-rule-decisions" = "error"
"ledger/root-exception-decisions" = "error"
```

In a single-project repository the root files are the project's: the project rules judge them, and
the `root-` rules report nothing, so no entry is reported twice.

## Baselines

```bash
bin/gate baseline write
```

Writes the fingerprints of today's findings to `gate/baseline.json`, replacing it; commit it. A finding
in the baseline is not reported; a baseline entry that matches no finding is, so the baseline only
shrinks. The summary line counts the findings the baseline holds back, as in
`0 errors, 0 warnings, 12 findings held back by gate/baseline.json`, so a clean run with a
baseline does not read as a clean tree; `--format json` has the count as `baselined`. Use it to adopt a rule on a repository with many existing findings, and exceptions for the
findings that stay on purpose.

```bash
bin/gate baseline prune
```

Once findings are fixed, prune removes their entries, the ones `gate check` reports as matching no
finding although their rule ran, and prints each one. An aspect set to `off` while its rule
runs reports nothing, so its entries are stale too, and prune deletes them. An entry written before
its plugin gave the finding a key is re-keyed instead: when the keyed finding has the entry's old
fingerprint (same rule, aspect, path and message), prune rewrites the entry with the key and prints
it. Prune never adds an entry: a new finding stays reported, and prune says how many there are.
`baseline write` is for the first baseline; running it again would accept every new finding too.
Prune reads the committed baseline; on a `gate/baseline.json` that git does not track it stops and
says to run `git add gate/baseline.json` first. Two baseline runs in one worktree take turns: the
second says it waits, and a prune then works from the baseline the first one wrote.

After moving a baseline written by 0.1.0 to 0.2.0, run `bin/gate baseline prune` first; it re-keys
every entry it can. One kind of finding changed in 0.2.0 so that prune cannot: a finding of a project
below the repository root that sits in one of the project's option files, or names one in its
message. 0.2.0 names such a path once (`apps/api/gate/options/<plugin>/<file>.json`, where 0.1.0
repeated the project's path), and a message names the file from the project's root
(`gate/options/<plugin>/<file>.json`). The entry's fingerprint no longer matches, so prune removes it
and prints it as removed, and the finding is reported again. Then check that nothing else is new: run
`bin/gate check` and pair each reported finding with an entry prune printed as removed, by rule,
aspect and message, the option file's path aside. When every reported finding pairs, run
`bin/gate baseline write` once and commit the baseline: it accepts again exactly the findings prune
removed. A reported finding that pairs with no removed entry is new: fix it, or make it an exception,
before writing, since `baseline write` accepts every finding it sees.

A fingerprint is the finding's rule, aspect and path with the plugin's stable key for it, or with its
message when the plugin gives no key. With a key, a plugin may reword a message without making the
entry stale.

## `[generate]`

```toml
[generate]
"agents/adapters" = { options = { root = "docs/agents" } }
```

Turns on a plugin's generator with its options (or `options-file`). A generator writes paths relative to
the project, and owns a fixed set of globs that its page names: `agents/adapters` owns
`.claude/rules/**` and `.claude/skills/**` of the project. `gate sync` writes the generated files and
deletes a tracked file in an owned glob that the generator no longer writes; `gate sync --check`
reports a file that differs from what the generator returns as a finding of `gate/sync`.

## `[steps]` and `[suites]`

```toml
[steps]
format = { run = "pnpm prettier --check", files = "staged", glob = "*.{ts,md}" }
lint = { run = "pnpm eslint .", timeout = 600 }
unit = { run = "pnpm vitest run", heavy = true, env = { CI = "1" } }

[suites]
static = ["format", "lint"]
all = ["format", "lint", "unit"]
```

| Step key | Meaning |
| --- | --- |
| `run` | the command |
| `cwd` | the working directory, relative to the root of the project the step runs for; defaults to that root |
| `timeout` | seconds before the step is stopped and failed |
| `env` | extra environment variables |
| `files` | `"staged"` or `"changed"`: the staged paths, or the paths changed since the base of the run, are appended to the command |
| `glob` | with `files`: only the paths that match this glob |
| `heavy` | run the command through the heavy-job wrapper (below) |
| `tools` | the `[tools]` the command calls; only these are installed and put first on its `PATH` |

A step in a project's file runs in that project's directory; a step in the root file runs at the
repository top. A suite runs every step in order, prints each exit code and time, and fails if any
step failed. Written as a table, it can stop at the first failure:

```toml
[suites]
ci = { steps = ["lint", "unit"], fail-fast = true }
```

`gate run <suite or step>` runs them; `--staged`, `--changed --base <rev>` and `--push <base>..<tip>`
limit the run to the projects the change touches.

`gate run <name> -- <argument>...` gives every step the arguments as `$1`, `$2`, ..., and
`gate run --stdin` gives every step what gate read on its standard input; otherwise a step's standard
input is empty. A hook's `run` entry passes the hook's own arguments and, in `pre-push`, the pushed refs
(below), so a `commit-msg` step reads the message file as `$1`.

A step marked `heavy` is prefixed by the command in `[run] heavy-wrapper` or `$GATE_HEAVY_WRAPPER`, for
example a machine-wide job queue:

```toml
[run]
heavy-wrapper = "scripts/queue.sh"
```

## `[tools]`

```toml
[tools.shellcheck]
version = "0.10.0"

[tools.shellcheck.targets.x86_64-unknown-linux-musl]
url = "https://github.com/koalaman/shellcheck/releases/download/v0.10.0/shellcheck-v0.10.0.linux.x86_64.tar.xz"
sha256 = "<the download's sha256, 64 lowercase hex digits>"
bin = "shellcheck-v0.10.0/shellcheck"

[tools.shellcheck.targets.aarch64-apple-darwin]
url = "https://github.com/koalaman/shellcheck/releases/download/v0.10.0/shellcheck-v0.10.0.darwin.aarch64.tar.xz"
sha256 = "<the download's sha256, 64 lowercase hex digits>"
bin = "shellcheck-v0.10.0/shellcheck"

[steps]
shell = { run = "shellcheck scripts/*.sh", tools = ["shellcheck"] }
```

An external tool, pinned in `gate.toml` itself by a sha256 per target triple; `version` is a label for
messages. Each target takes a `url` of an archive (`.tar.gz`, `.tgz`, `.tar.xz`, `.tar`, `.zip`) or
of the executable itself, `https`, or plain `http` to this machine only (`127.0.0.1`, `localhost`,
`[::1]`), as in `gate lock`; its `sha256`; and, for an archive, `bin`, the executable's path inside it.
A step names the tools it calls in `tools`: `gate run` downloads a step's tools for this machine just
before the step runs, once into the cache, verifies them, and puts them first on that step's `PATH`
only, so the step calls them by name. A tool that no step names is never fetched. Root file only.

## `[hooks]`

```toml
[hooks]
pre-commit = ["check --staged", "run format"]
commit-msg = ["check --commit-message"]
pre-push = ["check --push"]
budget = { warn-after = 3, fail-after = 10 }
```

Each git hook runs the listed `gate` commands in order: `check ...`, `run <step or suite> [--staged |
--push]` or `sync --check`, with no shell syntax. `--push` (in `pre-push` only) and `--commit-message`
(in `commit-msg` and the other hooks that receive a message file) take no value: the hook passes the
pushed ranges or the message file; a `check` with a message file checks only the message being
written: the enabled rules that declare they read it (`READS_MESSAGE`), such as `git/commit-message`.
It checks no tree, takes no `--staged`, `--changed`, `--push` or `--rev`, and its summary says
`commit message`. Every
`check` a hook runs also gets `--hook`, so rules that apply only
while a developer works (a prohibited branch name) know a hook started them; an entry, of a hook or a
job, does not write `--hook` itself. A `run` entry passes the hook's arguments to its steps (`-- "$@"`)
and, in `pre-push`, the pushed refs on their standard input (`--stdin`). `budget` gives every
`check` a hook runs its thresholds, `--warn-after` and `--fail-after` (see
[Time Budgets](#-time-budgets)); a `check` entry that writes either itself keeps exactly what it
writes. `gate sync` writes
the hooks to `.githooks/` and sets `core.hooksPath` once per clone; every worktree of the clone shares
it. `gate doctor` says when a clone has not run `gate sync`. Root file only.

### Hooks Written by Hand

A repository whose hooks already run through another manager, Husky for example, leaves `[hooks]`
out and calls the launcher from its own hook files. The two do not mix: with `[hooks]`, `gate sync`
points `core.hooksPath` at `.githooks`, away from the manager's directory; without it, `gate sync`
leaves `core.hooksPath` alone and `gate doctor` does not ask for it. The lines below are the ones
`gate sync` writes for `commit-msg = ["check --commit-message"]` and `pre-push = ["check --push"]`,
so a hand-written hook behaves as the generated one does.

`commit-msg` (in Husky, `.husky/commit-msg`):

```sh
#!/bin/sh
set -e
bin/gate check --commit-message "$1" --hook
```

git passes the hook the file that holds the message being written as `$1`; `--commit-message`
checks that message with the rules that read it, such as `git/commit-message`, besides the tree.

`pre-push` (in Husky, `.husky/pre-push`):

```sh
#!/bin/sh
set -e
refs=$(cat)
while read -r local_ref local_sha remote_ref remote_sha; do
  [ -n "$local_sha" ] || continue
  [ "$local_sha" != 0000000000000000000000000000000000000000 ] || continue
  bin/gate check --push "$remote_sha..$local_sha" --target-ref "$remote_ref" --hook </dev/null
done <<EOF
$refs
EOF
```

git writes one line per pushed ref on the hook's standard input: the local ref, its object id, the
remote ref and the remote's object id. The hook reads them all first and checks each ref on its own:

- `--push "$remote_sha..$local_sha"` checks the commits the push adds to that ref.
- `--target-ref "$remote_ref"` names the full remote ref the push updates, such as
  `refs/heads/feature/ab-12-login`. `git/branch-name` checks that name, so `git push origin
  HEAD:wip` checks `wip`, whatever branch is checked out.
- A local object id of zeros is a deleted ref: nothing is pushed, so the hook skips it. A remote
  object id of zeros is a new branch: the range is passed as it is, and the core takes a base of
  zeros as a new branch, where every path counts as changed.
- `</dev/null` keeps each check off the loop's standard input, which holds the refs still to check.

`--hook` says that a git hook started the run. Rules that apply only while a developer works read
it: `git/branch-name` refuses a name in its `prohibited` list (by default `main`, `master` and
`release`) only in a hook run, so a commit or a push from a working copy on `main` is refused,
while a plain `gate check` and a CI job, which check `main` out, pass. `set -e` makes the hook exit
with the first failing check's code, so git stops the commit or the push.
`[hooks] budget = { warn-after = 3, fail-after = 10 }` corresponds to `--warn-after 3 --fail-after 10`
before `--hook`, and `pre-commit = ["check --staged"]` to `bin/gate check --staged --hook`.

## `[ci.github]`

```toml
[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1"
branches = ["main"]
aggregate = "gate-passed"

[ci.github.jobs.gate]
name = "Gate"
runs-on = "ubuntu-24.04"
permissions = { contents = "read" }
timeout-minutes = 15
budget = { warn-after = 120, fail-after = 300 }
run = ["check --push"]

[ci.github.jobs.unit]
name = "Unit tests"
projects = ["server"]
timeout-minutes = 30
run = ["run unit --push"]
```

| `[ci.github]` key | Meaning |
| --- | --- |
| `workflow` | required: the workflow file, relative to the repository root |
| `checkout` | required: the checkout action pinned by commit, with its release after ` # `: `actions/checkout@<40 hex digits> # v7.0.1`; the workflow carries the release as a comment |
| `name` | the workflow's display name; defaults to `Gate` |
| `branches` | branch globs whose pushes, and pull requests into them, run the workflow; defaults to `["main"]` |
| `runs-on` | the runner of the generated jobs; defaults to `ubuntu-24.04` |
| `setup` | steps every job runs after the checkout, such as installing a toolchain |
| `aggregate` | the id of one more generated job that needs every other job, always runs, and fails unless each of them succeeded or was skipped: the single check branch protection requires |

A job keeps the id and the `name` you give it and takes `run` (the gate commands: `check ...`,
`run <step or suite> [--staged | --push]` or `sync --check`), `timeout-minutes` (required: the job's
time limit in minutes), `runs-on`, `permissions` (defaults to `{ contents = "read" }`), `environment`
(the deployment environment whose protection rules guard what the job writes), `services`, `steps`
(before the gate commands), `after` (after them, each with an optional `if`), `working-directory`,
`fetch-depth`, `budget` and `projects`. A step is an action pinned by commit (`uses`, `with`, the
release after ` # ` as for `checkout`) or a shell command (`run`, `env`, `working-directory`). In a
job, `--push` takes no value: the job passes the pull request's or the push's range.

The workflow meets the standard of the `ci` plugin's `ci/recommended` preset: top-level
`permissions: {}` and each job its own, a timeout on every job (15 minutes for `changes`, 5 for the
aggregate job), named steps, the aggregate job named by its id, which is the check name branch
protection requires. What it cannot decide for you the preset reports: an unnamed step of `setup`,
`steps` or `after` (`ci/names#step`); a pin without its release (`ci/actions-pinned#version-comment`);
a job that writes `contents`, `deployments`, `id-token` or `packages` without an `environment`
(`ci/environment`); a `workflow` file whose name is not the kebab case of `name`, such as
`.github/workflows/ci.yml` with the default name `Gate` (`ci/names#file`); a job id not in kebab case,
such as `lint_rust` (`ci/names#job-id`); a moving `runs-on` label, such as `ubuntu-latest`
(`ci/runner-pinned`).

The generated first job, `changes`, runs `gate sync --check` and lists the projects the change touches;
a job that lists `projects` is skipped when none of them changed. Generated `check` commands get
`--no-overrides` and the thresholds of the job's `budget`, exactly as set: a job with only
`warn-after` never fails on time. `gate sync` writes
the workflow; `gate sync --check` reports drift. Root file only.

The workflow's runs share a `concurrency` group per workflow file and ref
(`<workflow>@${{ github.ref }}`), so no other workflow shares it whatever its name. A newer run of a
pull request cancels the one still in progress for its older commit; a push to a branch never cancels
a running check of that branch, it waits for it. GitHub keeps one waiting run per group, so a third
push while one run works and one waits replaces the waiting run. There is no setting for this. The
`aggregate` job of a cancelled run still runs, since it runs always, and fails naming the cancelled
jobs: the superseded commit shows a red check, the new head does not. It runs always because a
skipped aggregate would count as passing.

The `bin/gate` steps of the workflow get `GATE_GITHUB_TOKEN`, the repository secret `GATE_GITHUB_TOKEN`
when it is set, else the job's own token, and `GATE_BRANCH`, the branch under test (a pull request's
checkout is a detached `HEAD`). Set the secret to a token that can read the releases of private
plugins. Actions and other steps get neither, and a `[steps]` command gets no `GATE_GITHUB_TOKEN` or
`GITHUB_TOKEN` unless its `env` sets one.

## `gate.lock`

Written by `gate lock`, committed, never edited by hand. It holds the resolved URL and sha256 of the
core for each platform and of every plugin, keyed by its source. Tools are pinned in `[tools]` itself.
Run `gate lock` after changing `[gate]` or any `[plugins]`; a run with a stale lock fails with exit
code `2` and says so.

A release job can pin the build it is about to publish without resolving the others:

```bash
bin/gate lock --set house=https://github.com/your-org/your-repo/releases/download/plugin-house-v1.3.0/house.wasm --file target/house.wasm
```

The pin gets the sha256 of the file and the cache gets the file, so `gate check` on this machine runs
exactly that build before the asset exists. Anywhere else, `gate check` downloads the asset and fails
closed if it is not the pinned file; when the asset is not found, the error says to publish it or to
admit the build with `--file`. A refused or missing credential keeps its own message, since the asset
may be private.

## ⏱ Time Budgets

These are options of `gate check`. A `budget` table, in `[hooks]` or a CI job, passes the two
thresholds to the checks it runs: `budget = { warn-after = <seconds>, fail-after = <seconds> }`,
with one key or both, each above 0, and `fail-after` above `warn-after`.

| Option | Effect |
| --- | --- |
| `--timings` | prints the time of each phase of the run (snapshot, configuration, selection, plugins, total) to standard error |
| `--warn-after <seconds>` | a run that takes longer is a `gate/budget` warning |
| `--fail-after <seconds>` | a run that takes longer is a `gate/budget` error, so the gate fails; above `--warn-after` when both are given (exit `2` otherwise) |
| `--fail-fast` | stops at the first error finding |

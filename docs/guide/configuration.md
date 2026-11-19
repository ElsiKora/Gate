# Configuration

Gate reads `gate.toml` files: one at the repository root, and one in each project of a monorepo that
needs settings of its own. This page describes every section. `gate schema` prints the JSON Schema of
the file, which editors use for completion through the `#:schema` line.

## 📄 Files

| File | Where | Holds |
| --- | --- | --- |
| `gate.toml` (root) | the repository root | `[gate]`, `[workspace]`, repository-scope plugins and rules, `[[profiles]]`, root steps and suites, `[run]`, hooks, CI, tools |
| `gate.toml` (project) | a project's root | `[project]`, that project's plugins, languages, rules, generators, steps and suites, `[exception-policy]` |
| `gate.exceptions.toml` | beside a `gate.toml` | the `[[exception]]` entries of that file's scope |
| `gate.lock` | the repository root, written by `gate lock` | the resolved URL and sha256 of the core per platform and of every plugin |
| `gate/baseline.json` | the repository root, written by `gate baseline write` | fingerprints of findings accepted for now |

When the root is the only project (no `[workspace]`, or a workspace that lists `"."`), the root file
also holds `[project]` and the project-scope rules.

A project gets its settings, in this order: the root's repository defaults, every profile whose
`projects` match it, then its own `gate.toml`. Each later source overrides the earlier ones. A project
with no file of its own is fully described by profiles, and a project's own file travels with the
project when it moves to another repository.

`gate list --origin` prints every setting of every project with the file and section it came from.

```toml
#:schema https://github.com/ElsiKora/Gate/releases/download/v0.1.0/gate.schema.json
```

## ✅ Validation

Gate validates every file before it runs anything. These are configuration errors, reported with the
file, the key and what is expected, with exit code `2`:

- an unknown section or key;
- a value of the wrong type;
- a rule id that no configured plugin declares;
- a project-scope rule in the root `[rules]` (the message points to `[[profiles]]`);
- options that fail the rule's JSON Schema, including unknown option keys;
- an exception without a `reason`, or without a `decision` when `[exception-policy]` requires one;
- a `gate.lock` that does not match the configuration.

## `[gate]`

```toml
[gate]
version = "0.1.0"
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
```

| Key | Meaning |
| --- | --- |
| `projects` | where projects come from: `"pnpm"` (the `packages` of `pnpm-workspace.yaml`), `"npm"` (the `workspaces` of the root `package.json`, which a yarn workspace uses too), or a list of globs such as `["apps/*", "packages/*"]` |
| `ignore` | globs of directories that never become projects; a `gate.toml` below them is not read. Rules still see their files: they belong to the enclosing project, or to the repository |

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

A project file, or the root file when the root is the only project; a profile takes `extends` for a set
of projects. A preset is a named set of rules with levels, levels of single aspects and options that
a plugin may ship, such as `git/recommended`. The project's own configuration wins over a preset. A
rule's level for one aspect comes, first found, from `[rules]` for that aspect, from `[rules]` for
the whole rule, from the rule's family, from the presets for that aspect, and from the presets for
the whole rule; else it is `error`. So a level `[rules]` gives the whole rule applies to all its
aspects, the ones a preset sets too: with `extends = ["git/recommended"]`,
`"git/commit-message" = "note"` reports `header-min-length` as a note, and `"off"` turns every aspect
off. A preset's aspect levels apply only where `[rules]` sets no level for that rule or aspect.

## `[plugins]`

```toml
[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.1.0" }
house = { source = "your-org/your-repo:house@1.2.0" }

[plugins.git]
source = "ElsiKora/Gate:git@0.1.0"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "0123456789abcdef0123456789abcdef01234567"  # the commit the policy starts after
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
ledger = { source = "ElsiKora/Gate:ledger@0.1.0", reads = ["docs/records"] }
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

A rule's page lists its options and its aspects.

The root `[rules]` holds repository-scope rules only (for example `git/no-ci-skip`). Project-scope
rules go in a project's own file or in a profile. `gate list` prints every rule that is on, every rule
or family that is off, and the file that turned it off.

## `[[profiles]]`

```toml
[[profiles]]
projects = ["apps/*"]

[profiles.plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.1.0" }
nestjs = { source = "ElsiKora/Gate:nestjs@0.1.0" }

[profiles.rules]
"typescript/parse" = "error"
"nestjs/barrel-exports" = "warning"

[[profiles]]
projects = ["packages/*"]

[profiles.plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.1.0" }
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
| `decision` | the id of the decision record that accepted it; required when `[exception-policy]` says so |
| `owner` | the record that owns the exception; a plugin may check that it is open |

`gate schema --exceptions` prints the JSON Schema of the file.

A finding that matches an exception is not reported. An exception that matches no finding is reported
as `gate/exceptions`, so exceptions are removed when they are no longer needed. There are no suppression
comments in source files, and data that exempts something is written as an exception, not as a rule
option.

A repository can make a decision mandatory on every exception:

```toml
[exception-policy]
require-decision = true
```

In the root `gate.toml` it applies to every `gate.exceptions.toml`; a project's own `gate.toml` can set
its own policy for its exceptions. The core enforces it when it loads the configuration: an exception
without `decision` is a configuration error. Whether the decision exists is a plugin's check: the
`ledger` plugin's `ledger/exception-decisions` rule reports a decision that its decision index does not
list as accepted.

## Baselines

```bash
bin/gate baseline write
```

Writes the fingerprints of today's findings to `gate/baseline.json`; commit it. A finding in the
baseline is not reported; a baseline entry that matches no finding is, so the baseline only shrinks.
Use it to adopt a rule on a repository with many existing findings, and exceptions for the findings
that stay on purpose.

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
budget = 10
```

Each git hook runs the listed `gate` commands in order: `check ...`, `run <step or suite> [--staged |
--push]` or `sync --check`, with no shell syntax. `--push` (in `pre-push` only) and `--commit-message`
(in `commit-msg` and the other hooks that receive a message file) take no value: the hook passes the
pushed ranges or the message file; a `check` with a message file checks the message being written
(the rules that read it, such as `git/commit-message`), and its summary says `commit message`. Every
`check` a hook runs also gets `--hook`, so rules that apply only
while a developer works (a prohibited branch name) know a hook started them; an entry, of a hook or a
job, does not write `--hook` itself. A `run` entry passes the hook's arguments to its steps (`-- "$@"`)
and, in `pre-push`, the pushed refs on their standard input (`--stdin`). `budget` applies
`--budget <seconds>` to hook runs: a hook that takes longer warns, it does not fail. `gate sync` writes
the hooks to `.githooks/` and sets `core.hooksPath` once per clone; every worktree of the clone shares
it. `gate doctor` says when a clone has not run `gate sync`. Root file only.

## `[ci.github]`

```toml
[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"   # v7.0.1
branches = ["main"]
aggregate = "gate-passed"

[ci.github.jobs.gate]
name = "Gate"
runs-on = "ubuntu-latest"
permissions = { contents = "read" }
budget = 300
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
| `checkout` | required: the checkout action pinned by commit, `actions/checkout@<40 hex digits>` |
| `name` | the workflow's display name; defaults to `Gate` |
| `branches` | branch globs whose pushes, and pull requests into them, run the workflow; defaults to `["main"]` |
| `runs-on` | the runner of the generated jobs; defaults to `ubuntu-24.04` |
| `setup` | steps every job runs after the checkout, such as installing a toolchain |
| `aggregate` | the id of one more generated job that needs every other job, always runs, and fails unless each of them succeeded or was skipped: the single check branch protection requires |

A job keeps the id and the `name` you give it and takes `run` (the gate commands: `check ...`,
`run <step or suite> [--staged | --push]` or `sync --check`), `runs-on`, `permissions` (defaults to
`{ contents = "read" }`), `services`, `steps` (before the gate commands), `after` (after them, each
with an optional `if`), `working-directory`, `fetch-depth`, `budget`, `projects` and
`timeout-minutes`. A step is an action pinned by commit (`uses`, `with`) or a shell command (`run`,
`env`, `working-directory`). In a job, `--push` takes no value: the job passes the pull request's or
the push's range.

The generated first job, `changes`, runs `gate sync --check` and lists the projects the change touches;
a job that lists `projects` is skipped when none of them changed. Generated `check` commands get
`--no-overrides`, and `--budget <seconds> --budget-fails` when the job sets `budget`. `gate sync` writes
the workflow; `gate sync --check` reports drift. Root file only.

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

A release job can move one pin without resolving the others:

```bash
bin/gate lock --set house=https://github.com/your-org/your-repo/releases/download/plugin-house-v1.3.0/house.wasm --sha256 <sha256>
```

## ⏱ Time Budgets

These are options of `gate check`.

| Option | Effect |
| --- | --- |
| `--timings` | prints the time of each phase of the run (snapshot, configuration, selection, plugins, total) to standard error |
| `--budget <seconds>` | warns when the run takes longer; `[hooks] budget` and a CI job's `budget` pass it |
| `--budget-fails` | makes an exceeded budget an error; the generated CI jobs pass it with a `budget`, hooks do not |
| `--fail-fast` | stops at the first error finding |

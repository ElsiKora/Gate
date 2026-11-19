# Monorepos

In a monorepo each project has its own plugins and rules, sets of projects share settings through
profiles, and a change runs only the projects it touches. This guide covers how Gate finds projects,
which project owns a path, where settings come from, what runs for a change, and how hooks and CI use
it.

## 🗂 Layout

```
repo/
├── gate.toml                  # [gate], [workspace], repository rules, [[profiles]], hooks, CI
├── gate.lock
├── bin/gate
├── pnpm-workspace.yaml
├── apps/
│   ├── server/
│   │   ├── gate.toml          # what only server needs
│   │   └── package.json
│   └── web/
│       └── package.json       # no gate.toml: described by profiles
└── packages/
    └── shared/
        └── package.json       # no gate.toml: described by profiles
```

The root `gate.toml`:

```toml
[gate]
version = "0.1.0"

[workspace]
projects = "pnpm"
ignore = ["samples/**"]

[plugins.git]
source = "ElsiKora/Gate:git@0.1.0"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "<the output of git rev-parse HEAD>"  # the commit the policy starts after
release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$'

[rules]
"git/no-ci-skip" = "error"

[[profiles]]
projects = ["apps/*", "packages/*"]

[profiles.plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.1.0" }

[profiles.rules]
"typescript/parse" = "error"

[profiles.steps]
lint = { run = "pnpm eslint ." }
unit = { run = "pnpm vitest run" }

[hooks]
pre-commit = ["check --staged"]
pre-push = ["check --push"]
```

The project file of `apps/server`, for what only that project needs:

```toml
[project]
name = "server"

[plugins]
nestjs = { source = "ElsiKora/Gate:nestjs@0.1.0" }

[rules]
"nestjs/import-direction" = { level = "error", options = { contracts = [{ source-directory = "src/modules/domain", aspect = "domain-direction", forbidden-owner-paths = ["src/modules/public"], forbidden-packages = ["@nestjs/swagger"] }] } }
```

`gate lock` at the root pins the plugins of every file in one `gate.lock`.

## 🔎 Finding Projects

`[workspace] projects` names where projects come from:

| Value | Projects |
| --- | --- |
| `"pnpm"` | the packages of `pnpm-workspace.yaml` |
| `"npm"` | the `workspaces` of the root `package.json` (a yarn workspace uses the same field) |
| `["apps/*", "packages/*"]` | every directory matching a glob |

Whatever the source, a directory with a tracked `gate.toml` is a project too. `[workspace] ignore`
lists directories that never become projects: a `gate.toml` below them is not read, but their files
are still seen, by the enclosing project or by repository rules. Cargo and Go workspaces are planned.

Only tracked files exist for Gate: an ignored directory, an untracked file or a nested worktree is never
a project and never read.

## 🏠 Which Project Owns a Path

A path belongs to the project with the longest root that contains it. `apps/server/src/main.ts` belongs
to `server`; `README.md` at the root belongs to no project and is seen by repository rules only.
Nested projects work the same way: `apps/server/tools/cli/` with its own `gate.toml` owns its files,
and `server` does not see them.

## 🧬 Profiles

A profile gives project settings to every project whose root matches its `projects` globs: `extends`,
`plugins`, `languages`, `rules`, `steps`, `suites`, `generate`. A profile that matches no project is a
configuration error. A project's settings come, in order, from:

1. the root's repository defaults;
2. every matching profile, in the order the root file lists them;
3. the project's own `gate.toml`, if it has one.

A later source overrides an earlier one, rule by rule and key by key. To see where each setting of each
project came from:

```bash
bin/gate list --origin
```

The root `[rules]` holds repository-scope rules only. A project-scope rule there is a configuration
error whose message points to `[[profiles]]`: put it in a profile with `projects = ["**"]` if every
project needs it.

## 📏 Which Rules Run Where

| Rule scope | Configured in | Runs | Sees |
| --- | --- | --- | --- |
| `repository` | the root `[rules]` | once per run | the whole repository |
| `project` | a profile or the project's own file | once per project that enables it | the project's own subtree, paths relative to the project root |

A rule declares its scope; the rule's page says it (`Scope: project`). A plugin that needs files of another
project gets them only through a read grant (`reads = [...]`, see
[configuration.md](configuration.md#plugins)).

## 🔀 What a Change Runs

```bash
bin/gate check --changed --base origin/main
bin/gate check --staged
bin/gate check --push origin/main..HEAD
```

1. Gate takes the changed paths: since the base, in the index, or in the range being pushed.
2. It selects the projects that own those paths.
3. It adds the projects that depend on them, from the package manager's workspace graph: a change to
   `packages/shared` also selects `apps/server` and `apps/web` if they depend on it.
4. Repository rules run when any path changed.

A dependent project's rule results come from the cache when its own inputs did not change; its steps
run again, because its dependency did. The report says which projects ran and why, and which were
skipped:

```text
ran server: depends on shared
ran shared: changed packages/shared/index.ts
skipped web: no change
gate check (changed since origin/main): 14 rules, 0 errors, 1 warning; projects: ran server, shared; skipped web
```

A plain `bin/gate check` checks every project.

## 🏃 Steps and Suites

Steps in a profile or a project file run in each project's directory; `bin/gate run unit` runs the
`unit` step of every selected project that defines it. Steps in the root file run once, at the
repository top, for work that is not about one project (a format check of the whole tree, a link check).
See [configuration.md](configuration.md#steps-and-suites).

## 🪝 Hooks and CI

The hooks are generated once, at the root, and run only the projects a commit or a push touches.
`core.hooksPath` is set once per clone, so every worktree of the clone uses the same hooks.

In CI, a generated first job runs `gate sync --check` and lists the projects the change touches; jobs
that need particular projects declare them and are skipped when none of them changed:

```toml
[ci.github]
workflow = ".github/workflows/gate.yml"
checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"   # v7.0.1
aggregate = "gate-passed"

[ci.github.jobs.gate]
name = "Gate"
run = ["check --push"]

[ci.github.jobs.unit]
name = "Unit tests"
projects = ["server", "web"]
run = ["run unit --push"]
```

In a hook or a job, `--push` takes no value: the hook passes each pushed range, the job the pull
request's or the push's range. `aggregate` adds one job that needs every other job and fails unless
each succeeded or was skipped, the one check for branch protection to require.

## 🚚 Moving a Project

A project's own `gate.toml` moves with the project. Settings it received from profiles stay behind:
`bin/gate list --origin` in the old repository shows which ones to carry over. In the new repository,
pin its plugins with `bin/gate lock`.

## ⚠️ Common Pitfalls

- **A project has no findings at all**: its files are not tracked yet, or its root is under
  `[workspace] ignore`. `bin/gate list` prints every project with its root.
- **A rule is unknown in a project**: the plugin is configured in another project's file or in a
  profile that does not match this project. `bin/gate list --origin` shows what applies.
- **A project rule in the root `[rules]`**: move it to a profile.
- **A dependent project is not checked**: the dependency is not declared in the package manager's
  manifest, so the workspace graph does not know it.

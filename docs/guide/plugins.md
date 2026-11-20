# Plugins

The shared plugins are released from this repository, each with its own version and changelog. Every
one is pinned the same way as a plugin you write yourself. This page says what each checks and how to
turn it on. Each rule's page, with its options, aspects and targets, is
`plugins/<plugin>/rules/<rule>.md` in this repository; `gate explain <plugin>/<rule>` prints what your
configuration says about the rule and where.

## 📦 Pinning a Plugin

```toml
[plugins.git]
source = "ElsiKora/Gate:git@0.2.1"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "<the output of git rev-parse HEAD>"  # the commit the policy starts after
release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$'
```

```bash
bin/gate lock
```

The key (`git`) is the prefix of the plugin's rule ids. The value is a table whose `source` is
`<owner>/<repo>:<plugin>@<version>`, with optional `options` shared by the plugin's rules, `options-file`
and `reads` ([configuration.md](configuration.md#plugins)). A source names the asset `<plugin>.wasm` of the release tagged `plugin-<plugin>-v<version>` of that repository: here
`git.wasm` of `plugin-git-v0.2.1` in `ElsiKora/Gate`. `gate lock` resolves it to a URL and a sha256 and
writes them to `gate.lock`. A plugin is one `.wasm` file for every platform.

Project-scope rules are configured in a project's `gate.toml` or in a `[[profiles]]` entry of the root
file; repository-scope rules in the root `[rules]` ([configuration.md](configuration.md#rules)).

## 🧩 The Shared Plugins

| Plugin | Scope | Languages | Reads history | Generators |
| --- | --- | --- | --- | --- |
| `typescript` | project | TypeScript, JavaScript | no | none |
| `git` | repository | none | yes | none |
| `markdown` | project | Markdown | no | none |
| `agents` | project | none | no | `agents/adapters`, `agents/agents-block` |
| `ledger` | project | none | no | none |
| `nestjs` | project | TypeScript | no | none |
| `ci` | repository | none | no | none |
| `docker` | project | none | no | none |
| `repository` | repository | none | no | none |

### `typescript`

The TypeScript language: `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`. Its rules check
that every tracked source parses. The
parser, module resolution (`tsconfig` paths, package exports) and source tables live in the TypeScript
language library, which other plugins link; see [writing-a-rule.md](writing-a-rule.md#5-languages).

```toml
[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.2.0" }

[rules]
"typescript/parse" = "error"
```

| Rule | Checks |
| --- | --- |
| [`typescript/parse`](../../plugins/typescript/rules/parse.md) | every tracked TypeScript or JavaScript source parses into a syntax tree |

### `git`

Commit history rules. They read the commits under review: the commit message in `commit-msg`, the
pushed range in `pre-push` and in CI.

```toml
[plugins.git]
source = "ElsiKora/Gate:git@0.2.1"

[plugins.git.options]
main = ["refs/remotes/origin/main", "refs/heads/main"]
activation = "<the output of git rev-parse HEAD>"  # the commit the policy starts after
release-subject = '^chore\(release\): (\d+\.\d+\.\d+)$'

[rules]
"git/no-ci-skip" = "error"
```

| Rule | Checks |
| --- | --- |
| `git/main-history` | every first-parent commit of main since activation follows the git policy |
| `git/no-ci-skip` | no commit since activation carries a CI-skip directive except the release commit |

### `markdown`

The documentation of a project: front matter, local links that resolve, and the sections a page must
have. By default a page is a `.md` file below the project's `docs`; `roots` and `extra-files` change
that.

```toml
[plugins]
markdown = { source = "ElsiKora/Gate:markdown@0.2.0" }

[rules]
"markdown/local-links" = { level = "error", options = { roots = ["docs"], extra-files = ["README.md"] } }
```

| Rule | Checks |
| --- | --- |
| [`markdown/frontmatter`](../../plugins/markdown/rules/frontmatter.md) | pages open with front matter that holds each required key |
| [`markdown/local-links`](../../plugins/markdown/rules/local-links.md) | every local link destination names a tracked file or directory |
| [`markdown/required-sections`](../../plugins/markdown/rules/required-sections.md) | pages have the level-two sections their path requires |

### `agents`

Rule files and skills written for coding agents, kept in `<root>/rules/*.md` and `<root>/skills/`
(`root` defaults to `docs/agents`), and the files generated from them. Paths are relative to the
project. `agents/adapters` writes one Claude Code adapter per rule file to `.claude/rules/<stem>.md` and
copies the skills to `.claude/skills/`; it owns `.claude/rules/**` and `.claude/skills/**`.
`agents/agents-block` writes the table of rule files between the `gate:begin agents` and
`gate:end agents` markers of `AGENTS.md` (`entry`).

```toml
# apps/server/gate.toml
[plugins]
agents = { source = "ElsiKora/Gate:agents@0.2.0" }

[rules]
"agents/rule-registry" = { level = "error", options = { root = "docs/agents" } }
"agents/no-claude-md" = "error"

[generate]
"agents/adapters" = { options = { root = "docs/agents" } }
"agents/agents-block" = { options = { root = "docs/agents", entry = "AGENTS.md" } }
```

`gate sync` writes the generated files; `gate sync --check` reports drift. The rules: `instruction-references`,
`no-claude-md`, `rule-enforcement`, `rule-file-size`, `rule-ids`, `rule-owner`, `rule-references`,
`rule-registry`, `skills` ([pages](../../plugins/agents/rules)).

### `ledger`

A decision ledger: Markdown records with ids, statuses and closers, the decision index, and the
decisions and owners exceptions cite. Every rule reads the shared `[plugins.ledger] options` (the
ledger's file paths, the closing statuses, the paired plan); see
[the plugin's page](../../plugins/ledger/rules/README.md).

```toml
# docs/gate.toml, the project that holds the ledger
[project]
name = "docs"

[plugins]
ledger = { source = "ElsiKora/Gate:ledger@0.2.0", options = { decisions = "ledger/decision-index.md" } }

[rules]
"ledger/decisions" = "error"
"ledger/records" = "error"
"ledger/exception-decisions" = "error"
```

| Rule | Checks |
| --- | --- |
| `ledger/decisions` | ADRs have their sections and linked decisions, and the decision index agrees with them |
| `ledger/manifest` | the ledger manifest is well formed and its files exist |
| `ledger/records` | records keep their fields, counterparts, commits, dependencies and closure evidence |
| `ledger/record-status` | a paired plan record, its implementation and its verification share one status, from `allowed` when set |
| `ledger/blockers` | a planned record names no closed blocker, and a started record no blocker |
| `ledger/closing-notes` | a temporary difference whose closer is closed carries a dated closing note |
| `ledger/reuses` | every open implementation record names the shared mechanisms it reuses |
| `ledger/exception-owners` | the owner an exception names is an open implementation record |
| `ledger/exception-decisions` | the decision an exception names is an accepted decision of the index; whether every exception needs one is `[exception-policy] require-decision` |

### `nestjs`

NestJS services built on [NestJS-Crud-Automator](https://github.com/ElsiKora/NestJS-Crud-Automator):
module boundaries, import direction, entity relations, field inventories, barrel exports. Links the
TypeScript language library.

```toml
# apps/server/gate.toml
[project]
name = "server"

[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.2.0" }
nestjs = { source = "ElsiKora/Gate:nestjs@0.2.1", options = { layout = { source-root = "src" } } }

[rules]
"nestjs/barrel-exports" = "error"
"nestjs/field-inventory" = { options-file = "gate/options/field-inventory.json" }

[rules."nestjs/import-direction"]
level = "error"
norm = "RULE-IMP-001"

[[rules."nestjs/import-direction".options.contracts]]
source-directory = "src/modules/domain"
aspect = "domain-direction"
forbidden-owner-paths = ["src/modules/public"]
forbidden-packages = ["@nestjs/swagger"]
```

Every rule reads the shared `layout` block of `[plugins.nestjs] options`
([its page](../../plugins/nestjs/rules/layout.md)); each rule's page is under
[plugins/nestjs/rules](../../plugins/nestjs/rules).

### `ci`

GitHub Actions workflows, generated or written by hand.

```toml
[plugins]
ci = { source = "ElsiKora/Gate:ci@0.2.0" }

[rules]
"ci/actions-pinned" = "error"
"ci/workflow-run-target" = "error"
```

| Rule | Checks |
| --- | --- |
| [`ci/actions-pinned`](../../plugins/ci/rules/actions-pinned.md) | actions and reusable workflows are pinned by a full commit SHA |
| [`ci/workflow-run-target`](../../plugins/ci/rules/workflow-run-target.md) | `workflow_run` triggers name workflows that exist |

### `docker`

Image files and compose files: pinned images, stages, a user that is not root, no secret written in,
ports on the loopback address only, and the health check where the orchestrator reads it.

```toml
[plugins.docker]
source = "ElsiKora/Gate:docker@0.3.0"
options = { registries = ["docker.io"] }  # optional, as are image-files, secret-names, secret-metadata-words

[project]
extends = ["docker/recommended"]
```

Both docker rules are project-scope: in a monorepo, put `extends = ["docker/recommended"]` in a
`[[profiles]]` entry for the projects that build images.

| Rule | Checks |
| --- | --- |
| [`docker/dockerfile`](../../plugins/docker/rules/dockerfile.md) | image files are named `Dockerfile`, pinned, staged, run as a user, carry the OCI labels and no health check, and ignore and hold no secret |
| [`docker/compose`](../../plugins/docker/rules/compose.md) | compose files and services are named for their purpose, images are pinned, secrets stay out outside end-to-end files, ports stay local, and awaited services declare a health check |

### `repository`

What no repository should track, whatever its languages.

```toml
# the root gate.toml of a single project
[plugins]
repository = { source = "ElsiKora/Gate:repository@0.3.0" }

[project]
extends = ["repository/recommended"]
```

`repository/recommended` sets both rules to `error` and `large-files#large` to `warning`: a file over
1 MiB is a warning, one over 5 MiB (`huge`) an error. Both rules are repository-scope: they run once
over the whole repository and are configured in the root file only; a profile or a project file
cannot turn them on. A monorepo root has no `[project]` and so extends no preset
([configuration.md](configuration.md#project)); it sets what the preset sets in its `[rules]`:

```toml
# the root gate.toml of a monorepo
[workspace]
projects = ["apps/*"]

[plugins]
repository = { source = "ElsiKora/Gate:repository@0.3.0" }

[rules]
"repository/tracked-secrets" = "error"
"repository/large-files" = { level = "error", levels = { large = "warning" } }
```

| Rule | Checks |
| --- | --- |
| [`repository/tracked-secrets`](../../plugins/repository/rules/tracked-secrets.md) | no environment file, key file, private key, token of a published shape or literal `.npmrc` credential is tracked; findings never hold the value |
| [`repository/large-files`](../../plugins/repository/rules/large-files.md) | no file over 1 MiB is tracked unless `extra` declares it with its job; over 5 MiB is `huge` |

## 🎛 Presets

A plugin may ship presets: named sets of rules with levels and options, turned on with
`extends = ["<plugin>/<preset>"]` in `[project]` or in a profile, which `[rules]` then overrides.
`docker` and `repository` ship two each: `<plugin>/recommended`, every rule at `error` (with
`repository/large-files#large` at `warning`), and `<plugin>/adoption`, every rule at `warning` for a
repository on its way there. Turn the other shared
plugins' rules on one by one in `[rules]`; `gate list` shows the result.

## 🔌 Plugins of Your Own

A plugin for your own conventions lives in your own repository and is pinned the same way:

```toml
[plugins]
house = { source = "your-org/your-repo:house@1.2.0" }
```

It resolves to `house.wasm` of the release `plugin-house-v1.2.0` of `your-org/your-repo`. A private
repository's plugin is fetched with `$GATE_GITHUB_TOKEN`, `$GITHUB_TOKEN` or `gh auth token`, in that
order. See [writing-a-plugin.md](writing-a-plugin.md).

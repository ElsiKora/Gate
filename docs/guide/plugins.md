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
| `docs` | project | none | no | `docs/layout` |
| `agents` | project; `no-claude-md` repository | none | no | `agents/adapters`, `agents/agents-block` |
| `ledger` | project | none | no | none |
| `nestjs` | project | TypeScript | no | none |
| `node` | repository, project | none | no | none |
| `cdk` | project | TypeScript | no | none |
| `ci` | repository | none | no | none |
| `docker` | project | none | no | none |
| `repository` | repository | none | no | none |

### `typescript`

The TypeScript language: `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`, and the rules
of any TypeScript code: every tracked source parses, and string variants are enums. The parser, module resolution (`tsconfig` paths, package exports) and source tables live in the TypeScript
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
| [`typescript/string-enums`](../../plugins/typescript/rules/string-enums.md) | string-literal variant contracts use enums, in every `.ts` file, with `test-roots` for the tests |

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

### `docs`

One documentation layout for every project, a standard per kind: `service`, `frontend`,
`infrastructure`, `library` and `monorepo-root`. The standards share their top-level names, index
files and order, and differ only where a kind's nature demands it, each difference with its reason.
A project names its kind in the plugin's options, which the rule and the generator both read; it
adds an entry of its own only by declaring it, with its job, in `extra`, and lists in `delegated`
the entries whose plugins it enables (`agents`, `ledger`), so the index links them.

```toml
[plugins]
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "service" } }

[rules]
"docs/layout" = "error"

[generate]
"docs/layout" = {}
```

`extends = ["docs/recommended"]` (every rule at error) or `["docs/adoption"]` (at warning) turns the
rule on in place of the `[rules]` line. `gate sync` seeds every required index page that is missing and never overwrites a page that exists;
`gate check` reports what the standard misses, and each seeded page until its marker line is
deleted. A monorepo root's `docs/` is checked when the workspace lists `"."` as a project with
`kind = "monorepo-root"`.

| Rule or generator | Checks or writes |
| --- | --- |
| [`docs/layout`](../../plugins/docs/rules/layout.md) (rule) | the entries of the kind, unknown and reserved names, declared extensions, the index's entry table and each index's sections |
| [`docs/layout`](../../plugins/docs/generators/layout.md) (generator) | the required index pages of the kind, as seeds |

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

[generate]
"agents/adapters" = { options = { root = "docs/agents" } }
"agents/agents-block" = { options = { root = "docs/agents", entry = "AGENTS.md" } }
```

`agents/no-claude-md` is a repository-scope rule: it runs once and sees every tracked path, so set it
in the root `gate.toml`'s `[rules]` (`"agents/no-claude-md" = "error"`); a project's file may not set
it.

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

### `node`

One Node toolchain for every project of a kind. It does not replace ESLint or Prettier: it makes the
package manager, the Node version, dependency versions, the Prettier and ESLint configuration, the
manifest fields, the script names and the TypeScript base the same everywhere, and the tools run as
Gate steps. Every value it checks is fixed by the plugin; the plugin option `kind` (`service`,
`frontend`, `infrastructure`, `library`, `monorepo-root`) selects the few that differ by kind, each
with its reason on the rule's page. A directory with an install of its own that is not a workspace
member (a deployment bundle, say) is declared in `separate` with its job; it is an install of its
own in every rule. `base` names the TypeScript base's path from an install root (default
`tsconfig.base.json`).

Two presets turn on all nine rules at one level: `node/recommended` at `error`, for a repository
that follows the standard, and `node/adoption` at `warning`, for one on its way to it. A project's
own `[rules]` still wins over either.

```toml
[project]
name = "api"
extends = ["node/adoption"]
```

```toml
# gate.toml at the repository root
[plugins]
node = { source = "ElsiKora/Gate:node@0.3.0", options = { kind = "monorepo-root" } }

[rules]
"node/package-manager" = "error"
"node/node-version" = "error"
"node/dependency-versions" = "error"
"node/formatter" = "error"
"node/linter" = "error"
"node/tsconfig-base" = "error"

[steps]
format = { run = "pnpm run format" }
lint = { run = "pnpm run lint" }
typecheck = { run = "pnpm run typecheck" }
test = { run = "pnpm run test", heavy = true }

[suites]
all = ["format", "lint", "typecheck", "test"]

[hooks]
pre-push = ["check --push", "run all --push"]
```

```toml
# apps/api/gate.toml
[project]
name = "api"

[plugins]
node = { source = "ElsiKora/Gate:node@0.3.0", options = { kind = "service" } }

[rules]
"node/package" = "error"
"node/scripts" = "error"
"node/tsconfig" = "error"
```

| Rule | Checks |
| --- | --- |
| [`node/package-manager`](../../plugins/node/rules/package-manager.md) | pnpm pinned at one exact version in `packageManager`; `save-exact=true`; one tracked `pnpm-lock.yaml` per install root and no other lockfile; every manifest is a member of its root's workspace |
| [`node/node-version`](../../plugins/node/rules/node-version.md) | one exact version in the root `.nvmrc`, in no other file; the root `engines.node` admits it and no older major; every other `engines.node` is the root's |
| [`node/dependency-versions`](../../plugins/node/rules/dependency-versions.md) | exact versions; in a workspace, external dependencies through the catalog and members through `workspace:*` |
| [`node/formatter`](../../plugins/node/rules/formatter.md) | one root `.prettierrc.json` with `printWidth` 120, `proseWrap` "preserve" and `trailingComma` "all", and nothing else; no `.editorconfig` key that changes what Prettier writes |
| [`node/linter`](../../plugins/node/rules/linter.md) | one flat root `eslint.config.mjs` on the shared configuration (`@elsikora/eslint-config`) and no other base; members import the root's |
| [`node/package`](../../plugins/node/rules/package.md) | the manifest fields of the kind: name, version, license, `"type": "module"`, `engines.node`; private, or published with its fields for a library; a private library is a workspace package with its `exports` |
| [`node/scripts`](../../plugins/node/rules/scripts.md) | the standard script names (`build`, `lint`, `lint:fix`, `typecheck`, `test`, `format`, `format:fix`); a bare name checks, `<name>:fix` writes; no synonym |
| [`node/tsconfig`](../../plugins/node/rules/tsconfig.md) | every configuration of a project extends its install root's base, turns no strict flag off, and resolves modules the kind's way |
| [`node/tsconfig-base`](../../plugins/node/rules/tsconfig-base.md) | each install root that uses TypeScript has its base, with every strict flag on and `NodeNext` resolution |

### `cdk`

AWS CDK apps written in TypeScript, one standard for all of them: the layout, file names and
declarations, `cdk.json` and `package.json`, accounts, regions and configuration, stack names,
construct ids, physical names, properties, tags, removal policies, cdk-nag 3, container health checks
and unit tests. The app is a
project of its own, its root the directory of `cdk.json`; the rules read the tracked files and never
run the app. Links the TypeScript language library.

```toml
# infrastructure/gate.toml
[project]
name = "infrastructure"
extends = ["cdk/recommended"]

[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.3.0" }
cdk = { source = "ElsiKora/Gate:cdk@0.3.0", options = { regions = ["eu-central-1", "us-east-1"] } }
```

A project extends the standard only through `[plugins.cdk] options`: `extra` directories and
`extra-tags`, each with its job, `regions` and `nag-packs`. An existing app starts from the
`adoption` preset, which keeps the renames at `warning`. The standard, the options and each rule's
page are under [plugins/cdk/rules](../../plugins/cdk/rules/README.md).

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
cannot turn them on. A monorepo root, which is not a project, extends the preset in `[workspace]`
([configuration.md](configuration.md#workspace)):

```toml
# the root gate.toml of a monorepo
[workspace]
projects = ["apps/*"]
extends = ["repository/recommended"]

[plugins]
repository = { source = "ElsiKora/Gate:repository@0.3.0" }
```

| Rule | Checks |
| --- | --- |
| [`repository/tracked-secrets`](../../plugins/repository/rules/tracked-secrets.md) | no environment file, key file, private key, token of a published shape or literal `.npmrc` credential is tracked; findings never hold the value |
| [`repository/large-files`](../../plugins/repository/rules/large-files.md) | no file over 1 MiB is tracked unless `extra` declares it with its job; over 5 MiB is `huge` |

## 🎛 Presets

A plugin may ship presets: named sets of rules with levels and options, turned on with
`extends = ["<plugin>/<preset>"]` in `[project]`, in `[workspace]` at a root that is not a project,
or in a profile, which `[rules]` then overrides; each place takes the preset's rules of the scope it
runs ([configuration.md](configuration.md#project)). Each plugin new in 0.3.0 ships two:
`<plugin>/recommended`, every rule at error, and `<plugin>/adoption`, every rule at warning (`docs`,
`docker` and `repository`; `repository/recommended` keeps `repository/large-files#large` at
`warning`); `cdk` ships `cdk/recommended` and `cdk/adoption`, which differs: it warns only on the
renames and keeps the other rules at error. `git` ships `git/recommended`. Turn the rules of the
other plugins on one by one in `[rules]`; `gate list` shows the result.

## 🔌 Plugins of Your Own

A plugin for your own conventions lives in your own repository and is pinned the same way:

```toml
[plugins]
house = { source = "your-org/your-repo:house@1.2.0" }
```

It resolves to `house.wasm` of the release `plugin-house-v1.2.0` of `your-org/your-repo`. A private
repository's plugin is fetched with `$GATE_GITHUB_TOKEN`, `$GITHUB_TOKEN` or `gh auth token`, in that
order. See [writing-a-plugin.md](writing-a-plugin.md).

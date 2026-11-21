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

| Plugin | Scope | Languages | Reads history | Generators | Presets |
| --- | --- | --- | --- | --- | --- |
| `typescript` | project | TypeScript, JavaScript | no | none | `typescript/recommended`, `typescript/adoption` |
| `git` | repository | none | yes | none | `git/recommended`, `git/adoption` |
| `markdown` | project | Markdown | no | none | `markdown/recommended`, `markdown/adoption` |
| `docs` | project | none | no | `docs/layout` | `docs/recommended`, `docs/adoption` |
| `agents` | project; `no-claude-md` repository | none | no | `agents/adapters`, `agents/agents-block` | `agents/recommended`, `agents/adoption` |
| `ledger` | project | none | no | `ledger/decision-counts`, `ledger/layout`, `ledger/review-progress` | `ledger/recommended`, `ledger/adoption` |
| `nestjs` | project | TypeScript | no | none | `nestjs/recommended`, `nestjs/adoption` |
| `node` | repository, project | none | no | none | `node/recommended`, `node/adoption` |
| `cdk` | project | TypeScript | no | none | `cdk/recommended`, `cdk/adoption` |
| `frontend` | project | TypeScript | no | none | `frontend/recommended`, `frontend/adoption` |
| `ci` | repository | none | no | none | `ci/recommended`, `ci/adoption` |
| `docker` | project | none | no | none | `docker/recommended`, `docker/adoption` |
| `repository` | repository | none | no | none | `repository/recommended`, `repository/adoption` |

### `typescript`

The TypeScript language: `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs`, and the rules
of any TypeScript code: every tracked source parses, a folder with a barrel is imported through
it, and string variants are enums. The parser, module resolution (`tsconfig` paths, package exports) and source tables live in the TypeScript
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
| [`typescript/barrel-imports`](../../plugins/typescript/rules/barrel-imports.md) | a folder with a barrel is imported from outside only through its barrel, and only for names it exports |
| [`typescript/string-enums`](../../plugins/typescript/rules/string-enums.md) | string-literal variant contracts use enums, in every `.ts` file, with `test-roots` for the tests |

Presets: `typescript/recommended` turns on all three rules, `typescript/parse`,
`typescript/barrel-imports` and `typescript/string-enums`, at `error`, and `typescript/adoption` the
same at `warning`.

### `git`

Commit messages, branch names and the history of main. They read the commits under review: the
commit message in `commit-msg`, the pushed range in `pre-push` and in CI. `git/recommended` turns on
`git/branch-name` with the template `:type/:ticket-:name` and `git/commit-message` with
`header-min-length` and `scope-max-length` at `warning`.

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
| [`git/commit-message`](../../plugins/git/rules/commit-message.md) | the message being written and each pushed commit's message have the conventional form; prohibited trailers and body lines are refused |
| [`git/branch-name`](../../plugins/git/rules/branch-name.md) | the branch a push updates, or the checked-out branch, has a name the template or pattern allows |
| [`git/main-history`](../../plugins/git/rules/main-history.md) | every first-parent commit of main since activation follows the git policy |
| [`git/no-ci-skip`](../../plugins/git/rules/no-ci-skip.md) | no commit since activation carries a CI-skip directive except the release commit |

Presets: `git/recommended` turns on `git/branch-name` with the template `:type/:ticket-:name` and
`git/commit-message` at `error`, and `git/adoption` the same at `warning`; `recommended` keeps
`header-min-length` and `scope-max-length` at `warning`, as commitlint does. Both leave out
`git/main-history` and `git/no-ci-skip`, which need plugin options only the repository can give:
`git/no-ci-skip` needs `activation` and `release-subject`, and `git/main-history` those two and
`branch-pattern`, `branch-length`, `dependabot` and `documentation`.

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

Presets: `markdown/recommended` turns on `markdown/frontmatter` and `markdown/local-links` at
`error`, and `markdown/adoption` the same at `warning`. Both leave out `markdown/required-sections`:
which page needs which sections only the project knows, and without `sections` the rule checks
nothing.

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

With `docs/layout`, `ledger/layout` and `agents/agents-block` in `[generate]` and
`delegated = ["agents", "ledger"]`, one `gate sync` lays down the whole standard: the indexes, the
ledger's manifest and shared files, `AGENTS.md` and the agents' reading map. `gate check` then
reports only what a person must write: each index's `#unfilled`, the rule files' `#input`, the
ledger files only the project can write (the ADR coverage, the catalog of `ledger/reuses`), and its
first implementation and production blocker records.

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
`gate:end agents` markers of `AGENTS.md` (`entry`), and seeds the entry, markers and table included,
when it is not tracked; with nothing tracked under `root`, it also seeds the reading map
`<root>/context-map.md`.

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

In a monorepo the service's `tsconfig.json` extends the root's base, so the plugin is granted it
with `reads`, as `cdk`, `frontend` and `typescript` are; without the grant the module resolver runs
without the base's `paths` and the plugin reports the base as a finding with the line to add. See
[configuration.md](configuration.md#languages).

`agents/no-claude-md` is a repository-scope rule: it runs once and sees every tracked path, so set it
in the root `gate.toml`'s `[rules]` (`"agents/no-claude-md" = "error"`); a project's file may not set
it.

`gate sync` writes the generated files; `gate sync --check` reports drift. An enabled rule or generator
whose input is missing fails: a rule that reads the rule files reports an empty `<root>/rules` as
`#input`, `instruction-references` reports a missing `entry`, and `agents-block` reports an empty
`<root>/rules` as `gate/sync#agents/agents-block`. The rules: `instruction-references`,
`no-claude-md`, `rule-enforcement`, `rule-file-size`, `rule-ids`, `rule-owner`, `rule-references`,
`rule-registry`, `skills` ([pages](../../plugins/agents/rules)).

Presets: `agents/recommended` turns on all nine rules at `error`, and `agents/adoption` the same at
`warning`.

### `ledger`

A decision ledger: Markdown records with ids, statuses and closers, the decision index, and the
decisions and owners exceptions cite. Every rule reads the shared `[plugins.ledger] options` (the
ledger's file paths, the closing statuses, the paired plan); see
[the plugin's page](../../plugins/ledger/rules/README.md). An enabled rule reports each file it reads
that is not tracked as `#input`, so a project without its ledger fails rather than passes; the ledger
manifest is the one list of what a complete ledger holds. `ledger/closing-notes` needs its `field`,
`ledger/reuses` its `catalog`, and `ledger/record-status` the shared `parity`: without them the run
stops with exit 2. The generator [`ledger/layout`](../../plugins/ledger/generators/layout.md) seeds
an empty ledger at the configured paths: the manifest and the five shared files. The rules then ask
only for content: the files the project writes (`#input`) and its first implementation and
production blocker records (`ledger/records`, keyed `#none`).

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
| `ledger/exception-decisions` | the decision an exception names is an accepted decision of the index; whether every exception needs one is `[decision-policy] require-decision` |
| `ledger/rule-decisions` | the decision a rule entry of the project's `gate.toml` names is an accepted decision of the index; the core hands the rule the decisions, so the plugin never reads `gate.toml` |

Presets: `ledger/recommended` turns on `decisions`, `manifest`, `records`, `blockers`,
`exception-owners`, `exception-decisions` and `rule-decisions` at `error`, and `ledger/adoption` the
same at `warning`. Both leave out `closing-notes`, `reuses` and `record-status`, which need the
project's `field` and `catalog` and the plugin option `parity`.

### `nestjs`

NestJS services built on [NestJS-Crud-Automator](https://github.com/ElsiKora/NestJS-Crud-Automator):
module boundaries, import direction, entity relations, field inventories, barrel exports. Links the
TypeScript language library.

```toml
# apps/server/gate.toml
[project]
name = "server"

[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.3.0", reads = ["tsconfig.base.json"] }
nestjs = { source = "ElsiKora/Gate:nestjs@0.3.0", options = { layout = { source-root = "src" } }, reads = ["tsconfig.base.json"] }

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

Presets: `nestjs/recommended` turns on every rule but the twelve below at `error`, and
`nestjs/adoption` the same at `warning`. Each of these takes something only the project can give;
turn it on in `[rules]` with its options:

| Rule | Why no preset turns it on |
| --- | --- |
| `nestjs/advisory-lock-boundary` | `executor` has no default |
| `nestjs/import-direction`, `nestjs/runtime-factory` | `contracts` has no default |
| `nestjs/notification-egress` | `owner-classes`, `egress.modules` and `owners` have no default |
| `nestjs/owner-boundaries` | the owner roots, owners and routes have no default |
| `nestjs/owner-roots` | `required-roots` has no default |
| `nestjs/raw-config-access` | `raw-imports` has no default |
| `nestjs/controller-owners` | the `controllers` registry is the project's own |
| `nestjs/field-inventory` | the `fields` inventory is the project's own |
| `nestjs/package-pins` | the `artifacts` it pins are the project's own |
| `nestjs/advisory-lock-manifest` | only a service that uses advisory locks has the manifest |
| `nestjs/asset-storage` | only a service that stores assets has the storage adapter |

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
typescript = { source = "ElsiKora/Gate:typescript@0.3.0", reads = ["tsconfig.base.json"] }
cdk = { source = "ElsiKora/Gate:cdk@0.3.0", options = { regions = ["eu-central-1", "us-east-1"] }, reads = ["tsconfig.base.json"] }
```

In a monorepo the app's `tsconfig.json` extends the root's base, so the plugin is granted it with
`reads`, as `nestjs`, `frontend` and `typescript` are; see
[configuration.md](configuration.md#languages).

A project extends the standard only through `[plugins.cdk] options`: `extra` directories and
`extra-tags`, each with its job, `regions` and `nag-packs`. An existing app starts from the
`adoption` preset, every rule at `warning`. The standard, the options and each rule's
page are under [plugins/cdk/rules](../../plugins/cdk/rules/README.md).

### `frontend`

React apps and libraries held to one standard: Feature-Sliced layers under `src/`, kebab-case names,
imports that point down through one alias, one module that talks to the network, one that reads the
environment, every user-visible string through the translation layer, assets and styles in known
places, one tool for each job. The option `pack` (`react-vite`, `nextjs` or `react-library`) names the framework; every rule
needs it. Nuxt and Vue are not covered. Links the TypeScript language library.

```toml
# apps/web/gate.toml
[project]
name = "web"
extends = ["frontend/recommended"]

[plugins]
typescript = { source = "ElsiKora/Gate:typescript@0.3.0", reads = ["tsconfig.base.json"] }
frontend = { source = "ElsiKora/Gate:frontend@0.3.0", options = { pack = "nextjs" }, reads = ["tsconfig.base.json"] }
```

In a monorepo the app's `tsconfig.json` extends the root's base, so the plugin is granted it with
`reads`, as `nestjs`, `cdk` and `typescript` are; see
[configuration.md](configuration.md#languages).

`frontend/recommended` sets every rule to `error`; `frontend/adoption` sets every rule to `warning`, for
an existing app moving to the standard. No preset can choose the framework, so a project that
extends either preset sets `[plugins.frontend] options pack` first; without it the check stops with
a configuration error that names the three packs. The plugin's page is
[plugins/frontend/rules/README.md](../../plugins/frontend/rules/README.md); each rule's page is beside it.

| Rule | Checks |
| --- | --- |
| `frontend/layout` | every file sits in the tree of the standard: layers, slices with `index.ts`, groups, segments |
| `frontend/file-names` | names under `src/` are kebab-case; route folders take their router's syntax |
| `frontend/typescript-sources` | no JavaScript under `src/` |
| `frontend/import-direction` | layers import only layers below them, and no sibling slice |
| `frontend/import-paths` | one alias, `@/`, through the public API; relative imports stay inside a slice |
| `frontend/component-names` | a `ui` file exports, by name, the component its name gives |
| `frontend/hook-names` | a hook lives in a file of its own name, in a `model` or `lib` segment |
| `frontend/state-placement` | stores are created in `model` segments or `app/store` |
| `frontend/api-boundary` | only `shared/api` talks to the network; only `api` segments import it |
| `frontend/env-access` | one module reads the environment, by literal public keys |
| `frontend/literal-text` | no literal user-visible text in JSX |
| `frontend/messages` | every locale has the same messages, none empty; the code uses only existing keys |
| `frontend/asset-placement` | imported assets sit in the `ui` segment of what shows them |
| `frontend/style-files` | global styles in one place; CSS modules beside their component |
| `frontend/route-files` | the route folder holds route files with their required exports |
| `frontend/client-boundary` | Next.js route files stay server components |
| `frontend/colocated-files` | tests and stories sit beside the file they are about |
| `frontend/import-alias` | `@/` for `src/` in the tsconfig and the Vite configuration, and no other alias |
| `frontend/route-config` | a Vite app runs TanStack Router's plugin on the standard's route folders |
| `frontend/packages` | one tool per job, and the router and translation library of the pack |

### `ci`

GitHub Actions workflows, generated or written by hand, held to one standard: every rule on at
`error`, the same in every repository; a workflow that must differ says so with an exception and its
reason in `gate.exceptions.toml`. The workflow `gate sync` writes from `[ci.github]` meets the
standard wherever it decides; what `[ci.github]` decides for it, a step's name, a pin's release, the
workflow's file name, a job id, the runner, the preset still reports
([configuration.md](configuration.md#cigithub) lists each).

A repository whose root is a project takes the standard through the preset `ci/recommended`:

```toml
[project]
name = "server"
extends = ["ci/recommended"]

[plugins]
ci = { source = "ElsiKora/Gate:ci@0.3.0" }
```

Every `ci` rule is repository-scope, configured in the root file only. A monorepo root, which has
no `[project]`, extends the preset in `[workspace]` ([configuration.md](configuration.md#project)):

```toml
# the root gate.toml of a monorepo
[workspace]
projects = ["apps/*"]
extends = ["ci/recommended"]

[plugins]
ci = { source = "ElsiKora/Gate:ci@0.3.0" }
```

| Rule | Checks |
| --- | --- |
| [`ci/actions-pinned`](../../plugins/ci/rules/actions-pinned.md) | actions and reusable workflows are pinned by a full commit SHA, with the release in a comment |
| [`ci/workflow-run-target`](../../plugins/ci/rules/workflow-run-target.md) | `workflow_run` triggers name workflows that exist |
| [`ci/permissions`](../../plugins/ci/rules/permissions.md) | every job sets its own token permissions; a workflow's own permissions only read |
| [`ci/concurrency`](../../plugins/ci/rules/concurrency.md) | every workflow has a concurrency group, and a newer run cancels only a pull request's |
| [`ci/timeout`](../../plugins/ci/rules/timeout.md) | every job that runs steps sets `timeout-minutes` |
| [`ci/runner-pinned`](../../plugins/ci/rules/runner-pinned.md) | jobs run on a versioned runner image, never `*-latest` |
| [`ci/checkout-credentials`](../../plugins/ci/rules/checkout-credentials.md) | every checkout sets `persist-credentials: false` |
| [`ci/secrets-in-steps`](../../plugins/ci/rules/secrets-in-steps.md) | secrets reach only the steps that use them |
| [`ci/environment`](../../plugins/ci/rules/environment.md) | a job that writes outside the checks runs in an environment |
| [`ci/node-setup`](../../plugins/ci/rules/node-setup.md) | `actions/setup-node` reads the version from a file, sets a cache and runs once per job |
| [`ci/names`](../../plugins/ci/rules/names.md) | every workflow, job and step is named; a file is its workflow's name in kebab case |

Presets: `ci/recommended` turns on all eleven rules at `error`, and `ci/adoption` the same at
`warning`.

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
runs ([configuration.md](configuration.md#project)). Every shared plugin ships two, as the table
above lists: `<plugin>/recommended` turns on its rules at `error`, for a repository that follows
the standard, and `<plugin>/adoption` turns on the same rules at `warning`, for one on its way
there. `git/recommended` keeps two aspects at `warning`, described under [`git`](#git), and
`repository/recommended` keeps `repository/large-files#large` at `warning`. A preset leaves out a
rule that needs something only the project can give, such as an option without a default or the
project's own inventory; each plugin's section above names those rules, and `[rules]` turns them on
with their options. Enabled from a preset, a rule whose files are missing reports the files to
create, never a configuration error for an option the project did not know about. One plugin
setting is the exception: every `frontend` rule needs `pack`, the project's framework, so the
`frontend` presets keep every rule and a project that extends either one sets
`[plugins.frontend] options pack` first. `gate list` shows the result.

```toml
[project]
name = "server"
extends = ["typescript/adoption", "nestjs/adoption"]
```

## 🔌 Plugins of Your Own

A plugin for your own conventions lives in your own repository and is pinned the same way:

```toml
[plugins]
house = { source = "your-org/your-repo:house@1.2.0" }
```

It resolves to `house.wasm` of the release `plugin-house-v1.2.0` of `your-org/your-repo`. A private
repository's plugin is fetched with `$GATE_GITHUB_TOKEN`, `$GITHUB_TOKEN` or `gh auth token`, in that
order. See [writing-a-plugin.md](writing-a-plugin.md).

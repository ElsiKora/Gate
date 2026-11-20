# T-065: One Node toolchain standard per kind, checked by a `node` plugin

| Field | Value |
| --- | --- |
| Status | round 276 green on `ed68f9a`; in review; the lead's rulings of 2026-11-20 being coded for one fix round |
| Release | v0.3.0 (a new plugin on `gate:plugin@0.3.0`; no interface change of its own) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | `main` at `ec66737`; the kind names and the `kind` option of T-062, so that both plugins read one line the same way |
| Touches | a new `plugins/node` (nine rules, the kinds as constants, one page per rule), `Cargo.toml` (the workspace member), `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

Every JavaScript or TypeScript project sets up its toolchain the same way: one package manager, pinned;
one Node version, pinned; one version per dependency; one formatter configuration and one linter
configuration per repository, imported rather than copied; one TypeScript base that every
configuration extends; and one script name per job. The standard differs by kind (a service, a
frontend, infrastructure, a library, a monorepo root) only where a kind's nature demands it, and each
difference carries its reason. A person who knows one repository's toolchain knows every other's.

## What was read for this design

Six repositories, read-only: tracked files listed and files read at `HEAD`, nothing run. No name,
path or text of theirs is in this repository.

- **Two backend services.** Each has three independent npm projects (the service, an infrastructure
  project, a load test) with three lockfiles and three different `engines.node` ranges. Their ESLint,
  Prettier and infrastructure TypeScript files are byte-identical between the two repositories:
  copied, not shared.
- **A pnpm monorepo** with 57 `package.json` and 97 `tsconfig*.json` files. It pins the package
  manager in `packageManager`, the Node version in a version file that agrees with `engines`, and one
  exact version per tool in the workspace catalog. One strict TypeScript base is extended by almost
  every configuration, the packages import the root ESLint configuration, and every package has the
  same four scripts. The exceptions are two projects moved in from the backends, which keep their own
  TypeScript, ESLint and Prettier files, and a spike with its own workspace and lockfile.
- **Three frontends**: one React application in JavaScript built with Vite, one React application in
  TypeScript built with webpack, and one Next.js application. None pins Node or the package manager,
  one tracks no lockfile, all three use ESLint 8 with a legacy configuration, and all three use `^`
  ranges.

The divergences that matter for the standard:

| Subject | What differs |
| --- | --- |
| Formatting | Five Prettier settings: indentation of one space, two spaces or tabs; a print width of 100, 120 or 480; single and double quotes; trailing commas "all" and "es5". Two of them sit inside one monorepo. |
| Script names | `format` checks in some projects and writes in others. Type checking is `typecheck` or `lint:types`, and one package has both. Lint is `lint`, `lint:ts` or both with the same command, and one `lint` rewrites files. |
| Pinning | One repository of six has a Node version file and a `packageManager` field. No frontend declares `engines`. |
| Versions | `^` ranges everywhere except the monorepo's catalog. `@types/node` majors 20, 25 and 26; TypeScript 5.1, 5.2 and 6.0; ESLint 8 and 10 side by side. |
| TypeScript | Outside the monorepo no configuration extends a base. `noUncheckedIndexedAccess` and `exactOptionalPropertyTypes` are on only in the monorepo's base; one frontend turns `noUnusedLocals` off. |
| Sharing | Configuration travels by copy: identical files across two repositories, into a monorepo's member, and across seven members of one monorepo. |

## 1. The plugin: a new `node` plugin

**Recommendation: a new plugin, `node`, that owns the toolchain files**: `package.json`, the lockfile,
the workspace file, the Node version file, `.npmrc`, the Prettier and ESLint configurations, and the
`tsconfig*.json` files. The alternative, rules in `typescript`, is not recommended:

- The subject is the toolchain, not the language. One of the three frontends is JavaScript; it needs
  the same package manager, Node version, versions, scripts and formatter rules, and has no use for a
  TypeScript parser.
- `typescript` is about source code (every file parses; imports through barrels). Folding the
  toolchain into it would make a project that wants its imports checked take the toolchain standard
  too, and the reverse. One plugin, one subject; a project turns each on alone (the T-062 argument
  for a separate `docs` plugin).
- The kinds must look alike. In one plugin, the side-by-side table below is one source file, and a
  difference without a reason fails a unit test.
- `tsconfig*.json` is a configuration file, read as JSON with comments. Its rule needs no parser
  from `gate-typescript`. The rule declares the language `typescript`, so it receives
  `[languages.typescript]` (the tsconfig path) like every plugin that declares it.

The core names no framework, no package manager and no tool; the plugin does, as plugins may.

## 2. The kinds: one table, alike, differences with reasons

A project picks its kind in one line, the same line and the same names as the `docs` plugin (T-062):

```toml
[plugins]
node = { source = "ElsiKora/Gate:node@0.3.0", options = { kind = "service" } }
```

- `kind` is required: `service`, `frontend`, `infrastructure`, `monorepo-root` or `library` (decided
  for both plugins). A missing or unknown kind is a configuration error (exit 2) that lists them. It is never
  inferred.
- In a monorepo each member sets its own kind, usually through a profile (`projects = ["packages/*"]`,
  `kind = "library"`). The root, listed as `"."`, sets `monorepo-root`.

A cell is **same** (as in every other kind), **absent** (not in this kind) or a value that differs, and
every cell that is not "same" has its reason. The rows are what the project-scope rules check.

| # | Subject | Service | Frontend | Infrastructure | Library | Monorepo root | Reason for each cell that is not "same" |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `name`, `version`, `license`, `type: "module"` | same | same | same | same | same | |
| 2 | `private: true` | same | same | same | absent, with `publishConfig` | same | Library: a published package must not be private, and says where it is published. |
| 3 | `exports`, `files`, `sideEffects`, `repository` | absent | absent | absent | required | absent | Library: what a consumer may import and what the tarball holds; nothing else is published. |
| 4 | `engines.node` | same | same | same | same | same | |
| 5 | scripts `build`, `lint`, `lint:fix`, `typecheck`, `test` | same | same | same | same | same | In a monorepo root each delegates to the workspace runner; the names do not change. |
| 6 | scripts `format`, `format:fix` | same | same | same | absent in a workspace member | same | Formatting is repository-wide with one configuration (section 3, `node/formatter`); a member that formats itself would format part of the tree with the root's settings anyway. |
| 7 | script `dev` | absent | required | absent | absent | absent | Frontend: a development server is part of the job. The others have no single such command; a service's `start:*` scripts are its own. |
| 8 | `typecheck` | same, when the project tracks a `tsconfig*.json` | same | same | same | same | A JavaScript project has nothing to type-check; the row applies only where TypeScript is configured. |
| 9 | `tsconfig` `module` / `moduleResolution` | `NodeNext` | `ESNext` / `Bundler` | `NodeNext` | `NodeNext` | `NodeNext` | Frontend: a bundler resolves the imports, so `Bundler` is the resolution that matches what runs. |
| 10 | the strict TypeScript flags (section 3) | same | same | same | same | same | |

A unit test of the plugin's constants keeps the kinds alike: the rows are the same set in every kind,
and every cell that is not "same" has a non-empty reason.

## 3. The rules

Nine rules (eight in the approved design; `node/tsconfig-base` by the lead's ruling of 2026-11-20).
The six repository-scope rules see every `package.json` and configuration file of the repository
once; the three project-scope rules read the project's kind. Every finding has a key (the
file and the field, script, flag or dependency), so its fingerprint survives a change of wording.

### `node/package-manager` (repository scope)

One package manager, pnpm, pinned at the repository root (decided: one way to do each thing).

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `field` | the root `package.json` has no `packageManager`, or it is not `pnpm@<x.y.z>` | the root `package.json` | `packageManager` |
| `lockfile` | `pnpm-lock.yaml` is not tracked at the root, or a lockfile of another manager (`package-lock.json`, `yarn.lock`, `bun.lock`, `bun.lockb`, `npm-shrinkwrap.json`) is tracked anywhere | the lockfile, or the root | the file |
| `nested` | a `pnpm-lock.yaml` or `pnpm-workspace.yaml` is tracked below the root, outside a declared separate root | the file | the file |
| `members` | a tracked `package.json` below an install root is in no package glob of that root's `pnpm-workspace.yaml`; with no workspace file, every such `package.json` is reported, since no install covers it | the `package.json` | the file |
| `save-exact` | the root `.npmrc` is not tracked, or does not set `save-exact=true` | `.npmrc` | `save-exact` |

Plugin option `separate` (in `[plugins.node] options`, next to `kind`): a list of `{ path, job }`,
each a directory that is its own install (a spike, a load test) with its own lockfile and workspace
file. It is the one extension, declared with its job like T-062's `extra`. `node/package-manager`
and `node/dependency-versions` check each separate root as an install root of its own; a `path` that
is not a plain relative directory, or an empty `job`, is a configuration error.

### `node/node-version` (repository scope)

One Node version, in one file, that every `engines.node` admits.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `file` | `.nvmrc` is not tracked at the root, or does not hold one exact version `x.y.z` | `.nvmrc` | `.nvmrc` |
| `form` | a second form is tracked: `.node-version`, a `nodejs` line in `.tool-versions`, `volta.node` in a `package.json` | the file | the file |
| `engines` | the root `package.json` has no `engines.node`, or its range does not admit the `.nvmrc` version, or admits an earlier major | the root `package.json` | `engines.node` |
| `differs` | another `package.json` declares an `engines.node` other than the root's | the `package.json` | `engines.node` |

### `node/dependency-versions` (repository scope)

One version per dependency, written exactly.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `range` | a version in `dependencies`, `devDependencies` or `optionalDependencies` is not exact (`^`, `~`, `*`, `x`, `latest`, a range, a tag) | the `package.json`, the line | the dependency |
| `catalog` | in a workspace, an external dependency of a member is not `catalog:` | the line | the dependency |
| `internal` | in a workspace, a dependency on a member is not `workspace:*` | the line | the dependency |
| `differs` | one dependency has two versions across the repository's `package.json` files (only without a catalog) | each line but the first in path order | the dependency |

`peerDependencies` may hold ranges in every kind: a peer range is what the consumer is allowed to bring.

### `node/formatter` (repository scope)

One Prettier configuration, data rather than code, at the root.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `form` | the configuration is anything other than a root `.prettierrc.json`: another file name, a `prettier` key in a `package.json`, or none | the file | the file |
| `nested` | a Prettier configuration is tracked below the root | the file | the file |
| `settings` | a key of `.prettierrc.json` differs from the standard, or the file holds a key the standard does not set | `.prettierrc.json`, the line | the key |

JSON is the one form because a rule can read it. A `.mjs` or `.cjs` configuration is code that a rule
cannot evaluate, so its settings could never be checked. The standard settings are one set for every
kind: `{ "printWidth": 120, "proseWrap": "preserve", "trailingComma": "all" }`, every other option at
Prettier's default. They are held once, as a constant of the plugin, because the shared ESLint
configuration fixes none (see "Decisions", item 1).

### `node/linter` (repository scope)

One flat ESLint configuration at the root, imported by the members and never copied.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `legacy` | an `.eslintrc*` file or an `eslintConfig` key is tracked | the file | the file |
| `root` | no `eslint.config.mjs` is tracked at the root, or a flat configuration with another name (`eslint.config.js`, `.cjs`, `.ts`) is tracked anywhere | the root, or the file | the file |
| `shared` | the root configuration does not import the `shared` package, or imports another configuration package as a base | the root configuration | the package |
| `detached` | a member's `eslint.config.mjs` does not import the root configuration by a relative path | the member's file | the file |

Option `shared`: the package name of the shared ESLint configuration, default `@elsikora/eslint-config`.
A project that uses ESLint extends it, and nothing else is a base: the root configuration imports it,
and importing another configuration package (`eslint-config`, `eslint-config-*`,
`@<scope>/eslint-config` or `@<scope>/eslint-config-*`) is `shared` too. Every repository's root then imports one package, which is how two repositories stop
copying the same file. The rule reads the import lines as text; it never evaluates the file.

### `node/package` (project scope)

The `package.json` of the project, against its kind (rows 1 to 4 and 9 of section 2).

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `missing` | a field the kind requires is absent | the `package.json` | the field |
| `forbidden` | a field the kind marks absent is present (`private: true` in a library; `workspaces` where pnpm owns the workspace) | the line | the field |
| `value` | a field holds another value than the kind's (`type` other than `"module"`) | the line | the field |

### `node/scripts` (project scope)

One name per job (rows 5 to 8).

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `missing` | a script the kind requires is absent | the `package.json` | the script |
| `synonym` | a script has a name whose job has a standard name: `lint:types`, `type-check`, `tsc`, `check-types` (`typecheck`); `format:check`, `prettier:check` (`format`); `prettier:fix`, `prettier:write`, `format:write` (`format:fix`); `lint:ts`, `eslint` (`lint`); `lint:ts:fix`, `eslint:fix` (`lint:fix`) | the line | the script |
| `writes` | a script without `:fix` passes `--fix` or `--write`. A bare name checks; `<name>:fix` writes. | the line | the script |
| `misplaced` | `format` or `format:fix` in a workspace member (row 6): a project with no `pnpm-lock.yaml` of its own | the line | the script |
| `name` | a script name is not kebab-case segments joined by `:` | the line | the script |

A project's other scripts (`start:api`, `migration:create`) are its own and need no declaration: the
script table is already where a script states its job, unlike a `docs/` directory, which is why T-062
asks for `extra` and this rule does not.

### `node/tsconfig` (project scope)

One base per repository; every configuration reaches it; no configuration weakens it.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `base` | a `tsconfig*.json` does not reach the repository's base through its `extends` chain | the file | the file |
| `weakened` | a configuration sets a standard flag to a weaker value (`strict: false`, `noUncheckedIndexedAccess: false`) | the line | the flag |
| `resolution` | `module` or `moduleResolution` differs from the kind's (row 9) | the line | the flag |
| `copy` | a configuration repeats the base's flags instead of extending it (no `extends`, and it sets three or more standard flags) | the file | the file |

The standard flags: `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`,
`noImplicitOverride`, `noImplicitReturns`, `noFallthroughCasesInSwitch`, `useUnknownInCatchVariables`,
`forceConsistentCasingInFileNames`, `isolatedModules` and `verbatimModuleSyntax`, all `true`. A project
may add any other flag (decorators, `paths`, `jsx`) and may be stricter. It may not be weaker except
through an exception, which names its reason.

Plugin option `base` (in `[plugins.node] options`): the base's path from an install root, default
`tsconfig.base.json`; a project's base is that of the install root that holds it. The files are
read as JSON with comments and trailing commas, as TypeScript reads them, so the rule declares no
language and needs no TypeScript plugin; a project with no `tsconfig*.json` has nothing to check.

### `node/tsconfig-base` (repository scope; the lead's ruling of 2026-11-20)

Each install root's base, checked once per install whether or not a Gate project owns it.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `missing` | an install root has a `tsconfig*.json` below it and its base is not tracked | the base's path | the file |
| `flags` | the base lacks a standard flag, or does not set `module` and `moduleResolution` to `NodeNext` | the base, or the line | the flag |

A `separate` install's base is a base of its own. `node/tsconfig` keeps project scope and checks
that each project's configurations reach their install root's base, none weakens a flag, and the
resolution is the kind's; it no longer checks the base's flags, so the base is checked in one place
per install.

## 4. One command runs ESLint, Prettier and the rest

Gate does not replace ESLint or Prettier. The rules above make their setup identical everywhere; the
tools themselves run as Gate steps, with the mechanism `main` already has: `[steps]` and `[suites]` in
`gate.toml`, run by `gate run`, and listed with `check` in the hooks and the CI job. No new mechanism.

```toml
[steps]
format = { run = "pnpm run format" }
lint = { run = "pnpm run lint" }
typecheck = { run = "pnpm run typecheck" }
test = { run = "pnpm run test", heavy = true }

[suites]
static = ["format", "lint", "typecheck"]
all = ["format", "lint", "typecheck", "test"]

[hooks]
pre-commit = ["check --staged", "run static --staged"]
pre-push = ["check --push", "run all --push"]

[ci.github.jobs.gate]
name = "Gate"
run = ["check --push", "run all --push"]
```

- The step names are the standard script names, so a step is one line that calls the script. In a
  monorepo the root's steps call the workspace runner through the root scripts; `--staged` and
  `--push` (in a CI job, the pull request's or the push's range) limit a run to the projects the
  change touches. CI installs pnpm and Node in the job's `setup`.
- The shared ESLint configuration runs Prettier as an ESLint rule (`withPrettier`), with the
  repository's `.prettierrc.json`. So `lint` also fails on unformatted code, and `format` remains the
  step that checks the files ESLint does not lint (Markdown, YAML, JSON).
- What `main` has today: `gate check` runs the rules; `gate run <suite>` runs the steps; a hook or a CI
  job lists both, so the developer's commit, push and the CI job are each one command. `bin/gate check`
  on its own does not run steps. Making it do so would be a new mechanism, which this task does not
  add; it is the lead's call (see "Decisions", item 5).

## 5. What a project may extend

- A separate install root, declared in `node/package-manager.separate` with its job.
- Scripts beyond the standard, under the name grammar and outside the synonyms.
- TypeScript flags beyond the standard, and stricter values of the standard ones.
- Dependencies, of course, at one exact version each.

A project may not choose another package manager, another formatter setting, a second formatter or
linter configuration, or another name for a standard job. Those are what make repositories alike. A
project moving to the standard uses levels, exceptions with reasons, or the baseline, as with any rule.

## 6. Tests

Native SDK tests on synthetic trees, one fixture per aspect with a passing twin, plus the kinds' unit
test. Each case is one that would let a broken change through, or block a good one.

- **The kinds** (a unit test of the constants): the rows are the same in every kind; every cell that
  is not "same" has a reason; `format` is absent only in a workspace member; the synonym table names
  only standard targets.
- **Configuration**: `kind` missing, or `"backend"`, exits 2 listing the five; a `separate` entry
  without `job`, or with a path outside the repository, exits 2 naming the key.
- **`node/package-manager`**: a root with `pnpm@9.1.0`, `pnpm-lock.yaml` and `save-exact=true` passes;
  `packageManager` absent, or `pnpm@^9`, is `field`; a `package-lock.json` in a member is `lockfile`; a
  nested `pnpm-lock.yaml` is `nested` unless its directory is declared `separate`; a `package.json`
  outside every workspace glob is `members`.
- **`node/node-version`**: `.nvmrc` `24.11.0` with `engines.node` `>=24.11.0 <25` passes; `.nvmrc` `24`
  is `file`; `engines` `>=22` is `engines` (admits an earlier major); a member with `>=24` is `differs`;
  a `.node-version` beside `.nvmrc` is `form`.
- **`node/dependency-versions`**: `"1.2.3"` passes and `"^1.2.3"`, `"~1.2.3"`, `"*"`, `"latest"` are each
  `range`; a peer `"^18"` passes; in a workspace, `"1.2.3"` in a member is `catalog` and a member
  dependency `"1.0.0"` is `internal`; without a catalog, `left-pad` at two versions is one `differs`.
- **`node/formatter`**: the standard `.prettierrc.json` passes; one key changed is `settings` at that
  key's line; `prettier.config.mjs` is `form`; a member's `.prettierrc.json` is `nested`.
- **`node/linter`**: a root `eslint.config.mjs` importing the default `shared` package passes; one that
  does not import it, or also imports `eslint-config-other`, is `shared`; with the option set to another
  name, that name is the one required; `.eslintrc.json` is `legacy`; a member that does not import
  `../../eslint.config.mjs` is `detached`.
- **`node/package`**: one fixture per kind passes; a library with `private: true` is `forbidden`; a
  service without `type` is `missing`.
- **`node/scripts`**: each synonym is reported with its standard name in the message; `lint` running
  `eslint --fix .` is `writes`; `format` in a workspace member is `misplaced`; a frontend without
  `dev` is `missing`; `start:api` passes.
- **`node/tsconfig`**: a chain `tsconfig.build.json` -> `tsconfig.json` -> base passes; a file with no
  `extends` and four standard flags is `copy` and `base`; `strict: false` in a member is `weakened`; a
  frontend with `NodeNext` is `resolution`; comments and trailing commas parse.
- **End to end**, once: a synthetic pnpm workspace with a root, a service and a library, run through
  the built component: `gate check` exits 0, and exits 1 with exactly the expected findings after one
  change of each kind.

## Decisions (the lead, 2026-11-19)

1. **Prettier.** The rule was: if the public package `@elsikora/eslint-config` fixes Prettier options
   or ships a Prettier configuration, it is the standard and Gate holds no second copy. **It does
   not.** Version 3.10.2 (the registry's `latest`, read from its public metadata and file listing):
   none of its 44 ES modules sets a Prettier option, and its Prettier part only loads
   `eslint-plugin-prettier/recommended`, which reads the project's own configuration. So the plugin
   holds the standard: `printWidth` 120, `trailingComma` `"all"`, `proseWrap` `"preserve"`, the rest
   at Prettier's defaults.
2. **`library`**: a kind of this plugin and of `docs` (T-062).
3. **pnpm** is the one package manager.
4. **The shared ESLint configuration** is `@elsikora/eslint-config`, the default of `node/linter.shared`.
   A project that uses ESLint extends it, and nothing else is a base.
5. **One command.** Gate makes ESLint and Prettier identical everywhere and runs them; section 4 says
   how, with `main`'s steps, suites, hooks and CI jobs. Whether `bin/gate check` alone should also run
   steps is open: it is a new mechanism, outside this task.

## Done when

- The plugin, its nine rules, the kinds and a page per rule are on a branch from `main`, with the
  tests of section 6, each failing with its change taken out.
- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the
  release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Proof

Lane ticket 276 on `ed68f9a`, 2026-11-20T01:14:15+00:00 to 2026-11-20T01:26:02+00:00, `one-gate` exit
0 (`round-inner.sh`):

| Step | Command | Exit |
| --- | --- | --- |
| fmt | `cargo fmt --all --check` | 0 |
| clippy | `cargo clippy --offline --locked --workspace --all-targets -j 4 -- -D warnings` | 0 |
| test | `cargo test --offline --locked --workspace -j 4 --no-fail-fast` | 0, 1098 passed, 0 failed |
| deny | `cargo deny --offline --locked check` (advisories, bans, licenses, sources ok) | 0 |
| release build | `cargo build --release --offline --locked -j 4` | 0 |

The plugin's tests are its 13 unit tests and the 48 tests of `plugins/node/tests/node`; the end to end
test `crates/gate/tests/node.rs` runs the built component on a synthetic workspace (a root, a service
and a library) and passed: exit 0 on the standard tree, exit 1 with one finding per rule after one
change against each.

Each mutation, applied to the round's tree, made its exact test fail (exit 101):

| Mutation | Test |
| --- | --- |
| `dependency_versions.rs`: the exactness check of an install root without a workspace never fires | `dependency_versions::without_a_workspace_every_installed_version_is_exact_and_peers_may_range` |
| `scripts.rs`: the synonym table is empty | `scripts::a_synonym_of_a_standard_name_is_reported_with_that_name` |
| `tsconfig.rs`: the `extends` chain is never followed to the base | `tsconfig::a_configuration_that_repeats_the_flags_instead_of_extending_is_a_copy` |

Rounds 270 and 272 to 275 stopped at clippy or at one test; each fix is its own commit on the branch.

### Rulings after round 276 (the lead, 2026-11-20)

1. **The base's flags without a root project**: a ninth rule, `node/tsconfig-base`, at repository
   scope, reads each install root's base directly; `node/tsconfig` drops `flags`. `base` moves to
   the plugin options, since both rules read it. Done, with tests and a page.
2. **`separate`** is an install of its own in every rule: `node/node-version` takes each install
   root's own `.nvmrc`, and `engines.node` below it agrees with that. Done, with a test.
3. **Limits**: a `catalog:` reference must name an entry of that catalog (done, with a test); an
   `extends` resolves as TypeScript resolves it, as written, then with `.json`, then a directory's
   `tsconfig.json` (done, with a test); a pre-release is exact in every rule (done, one function,
   with a test); a fixture's `package.json` and lockfiles stay exempt through `separate` only, and
   the `node/package-manager` page says so.
4. **Changelog**: the entry is under `# 0.3.0 (unreleased)`.

Gate 007's frontend ESLint options and the independent review's findings join these in one fix
round; no round is queued until both are in.

## Resume point

2026-11-20T01:31:15+00:00: rulings 2 to 4 coded on the branch, not yet compiled (no round, as the lead asked). Waiting
for: the lead's answer on ruling 1, Gate 007's ESLint options, the review's findings. Then one fix
round.

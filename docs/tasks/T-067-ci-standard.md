# T-067: A standard for CI workflows, checked by the ci plugin

| Field | Value |
| --- | --- |
| Status | design, for the lead's review |
| Release | v0.3.0 |
| Writer | "Gate 006" |
| Reviewer | the lead |
| Depends on | `main` at `ec66737` |
| Touches | `plugins/ci/src/*` (new rules, the `recommended` preset), `plugins/ci/rules/*.md`, `plugins/ci/tests/ci/*`, `plugins/ci/Cargo.toml` (0.3.0), `plugins/ci/CHANGELOG.md`, `crates/gate-host/src/sync/github.rs`, `crates/gate-host/src/sync/yaml.rs`, `crates/gate-host/src/config/file.rs` (`CiJob`), `crates/gate-host/src/plugins.rs` (`SYNC_STATE_VERSION` only), `crates/gate/tests/cli.rs`, `docs/guide/configuration.md`, `docs/architecture.md` section 10, `CHANGELOG.md` |

## Goal

The repositories that use Gate write their GitHub Actions workflows each in its own way. A
read-only survey of three of them (`git ls-files .github` and `git show HEAD:<file>` only; the
table of divergences stays outside this repository) found the same concern solved three ways, or not
at all:

- jobs without their own `permissions` (13 of 16 jobs in one repository, 11 of 13 in another), and
  a write scope granted to a whole workflow;
- no `timeout-minutes` on most jobs (12 of 16, 12 of 13, and every job of a workflow `gate sync`
  wrote, since the generator writes one only when it is set);
- a `concurrency` group that cancels a running push check of the default branch, and workflows
  with none;
- `ubuntu-latest` on every hand-written job;
- actions pinned by SHA everywhere, but the version comment major-only (`# v4`) or, in the
  generated workflow, absent;
- jobs with write scopes that leave the repository (a release, a package publish) without an
  `environment`;
- a secret in a job-level `env`, where every step of the job sees it;
- Node and its package manager set up three different ways, in one job twice;
- unnamed jobs and steps, among them the generated checkout step and aggregate job; job ids in
  `snake_case` beside kebab-case.

This task writes one standard, checks it with rules of the `ci` plugin bundled in one preset, and
makes the workflow `gate sync` writes satisfy it. v0.3.0 is the last release for a long time, so
the standard is set once, here.

## The standard

Each item is a rule (or an aspect of one) below; nothing in the standard is checked by hand.

1. **Permissions, least privilege.** The workflow's top-level `permissions` is `{}` or read-only
   scopes. Every job sets its own `permissions`. A write scope appears only on the job that needs
   it. `read-all` and `write-all` are never used.
2. **Concurrency.** Every workflow has a top-level `concurrency` with a `group`. A run is cancelled
   in progress only for a pull request: `cancel-in-progress` is `false` or an expression (the
   generator's `${{ github.event_name == 'pull_request' }}`), never a literal `true` in a workflow
   that runs on anything but pull requests. A push to the default branch, a deploy and a publish
   are never cancelled by a newer run.
3. **Timeouts.** Every job that runs steps sets `timeout-minutes` (a job that calls a reusable
   workflow cannot, and is not asked to).
4. **Runners.** `runs-on` names a versioned image (`ubuntu-24.04`), never a moving label
   (`ubuntu-latest`): a runner image change is a commit, like an action update.
5. **Actions.** Every `uses:` is pinned by a full commit SHA (as today) and carries the release it
   is, in full, in a comment: `uses: actions/checkout@<sha> # v4.2.2`. A major-only comment tells
   a reviewer nothing about which release the SHA is.
6. **Checkout.** Every checkout sets `persist-credentials: false`. A job that must push (a release
   commit and tag) is an exception in `gate.exceptions.toml` with its reason, not an in-file
   comment of another tool.
7. **Secrets.** A secret is referenced only in the step that needs it (`env` or `with` of a
   step), never in a workflow- or job-level `env`, and a reusable workflow is never passed
   `secrets: inherit`.
8. **Environments.** A job that holds a write scope reaching outside the checks (`contents`,
   `deployments`, `id-token`, `packages`) runs in an `environment`, so its protection rules
   (reviewers, branch policy, environment secrets) apply.
9. **Node.** Where a workflow sets up Node, `actions/setup-node` reads the version from a file
   (`node-version-file`), sets `cache`, and runs once per job; the package manager is installed by
   one step, the same in every workflow of the repository.
10. **Names.** Every workflow, job and step has a `name`. A workflow's file name is its name in
    kebab case (`deploy-staging.yml` for `Deploy Staging`). Job ids are kebab case. Each job has one
    name, unique in its workflow, and that name is what branch protection requires, so it never
    changes when the job's steps do.
11. **Gate in CI.** Gate runs in CI only through the workflow `gate sync` writes from
    `[ci.github]`; a repository writes no second workflow that builds or calls Gate.

Exceptions to any rule go in `gate.exceptions.toml` with a reason, the one way Gate has.

## The principles this follows

- **Packs alike.** Every repository takes the standard the same way: `extends =
  ["ci/recommended"]` in the root `gate.toml`, one preset with the same levels everywhere, and no
  per-repository list of rule levels. A repository that needs to differ says so with an exception
  and a reason, which a reviewer sees. The generated workflow and the hand-written ones are held to
  the same rules; nothing is exempt for being generated.
- **One name per job.** A job has one name: its `name:`. It is unique in the workflow, it is the
  check name branch protection requires, and the generator never derives a second one. Where the
  generator writes a job, the name is the one `[ci.github]` gives (`jobs.<id>.name`), and for the
  aggregate job, which has no configured name, its id: the check name it has today, so no branch
  protection rule changes.

(Reading of "packs alike, one name per job" above is the writer's; see Open questions.)

## New rules in the ci plugin

All are repository scope, read no history, and share the plugin's `workflows` option (the glob,
default `.github/workflows/*.{yml,yaml}`). Each reports at the line of the offending node, with a
target of `<path>:<job id>` (or `<path>` for a workflow-level finding), so an exception names one
job. Each gets a page in `plugins/ci/rules/`.

| Rule | Aspects | Options (besides `workflows`) | Reports |
| --- | --- | --- | --- |
| `ci/permissions` | `workflow-write`, `job-missing`, `shorthand` | none | a write scope in top-level `permissions`; a job without `permissions`; `read-all`/`write-all` anywhere |
| `ci/concurrency` | `missing`, `cancel` | none | a workflow without top-level `concurrency.group`; a literal `cancel-in-progress: true` in a workflow with a trigger other than `pull_request`, `pull_request_target` or `merge_group` |
| `ci/timeout` | `missing`, `over-max` | `max-minutes` (integer, unset by default) | a job with `steps` and no `timeout-minutes`; one above `max-minutes` when set |
| `ci/runner-pinned` | none | `moving` (globs, default `["*-latest"]`) | a `runs-on` label (string or list item) that matches `moving`; an expression is not resolved and passes |
| `ci/actions-pinned` (existing) | new `version-comment`; the pin finding keeps the empty aspect, so its fingerprints do not change | new `version-comment` (regex, default `^v\d+\.\d+\.\d+$`) | a SHA-pinned `uses:` whose line has no comment matching the pattern |
| `ci/checkout-credentials` | none | `actions` (default `["actions/checkout"]`) | a step using one of `actions` without `persist-credentials: false` |
| `ci/secrets-in-steps` | `workflow-env`, `job-env`, `inherit` | none | `secrets.` in a top-level or job-level `env`; `secrets: inherit` on a reusable-workflow call |
| `ci/environment` | none | `scopes` (default `["contents", "deployments", "id-token", "packages"]`) | a job granting `write` on one of `scopes` without `environment` |
| `ci/node-setup` | `version-file`, `cache`, `once` | none | `actions/setup-node` with `node-version` instead of `node-version-file`; without `cache`; twice in one job |
| `ci/names` | `workflow`, `file`, `job`, `step`, `duplicate`, `job-id` | none | a missing `name` (workflow, job, step); a file name that is not the kebab case of the workflow's name; two jobs of a workflow, or two workflows, with one name; a job id not in kebab case |

`ci/recommended` turns on every rule above at `error`, with the existing `ci/workflow-run-target`.

Notes for the code:

- The YAML reader (`yaml.rs`, `yaml_rust2`) drops comments. `version-comment` reads the source
  line of the `uses:` node from the tree's bytes (the node keeps its line) and takes the text after
  ` #`.
- `ci/names#file` derives kebab case from the name's words (`Release And Publish` →
  `release-and-publish`); a name holding an expression (`${{ ... }}`) is not checked for `file` or
  `duplicate`, since it differs per run.
- `ci/node-setup` names an action, not a language: the core still names none. A repository without
  Node has no `actions/setup-node` step and no finding.
- Adding aspects to `ci/actions-pinned` keeps every existing finding's fingerprint (aspect empty,
  same rule, path and key), so stored results and exceptions stay valid.

## How it fits the generated workflow: no second source

What Gate writes into a workflow is decided in one place, the generator in
`crates/gate-host/src/sync/github.rs`, from `[ci.github]`. The rules check properties of every
workflow file; they never restate a value `[ci.github]` holds (the checkout SHA, the runner, the
job names), and the generator never consults the rules. The tie between them is one test: a
workflow generated from a `[ci.github]` that uses every key passes `ci/recommended` with no finding.
If the generator drifts from the standard, that test fails; if the standard changes, the test shows
what the generator must change.

Changes to the generator so its output passes:

1. The checkout step gets a name (`Check out the repository`).
2. The aggregate job gets `name: <its id>`: the same check name as today.
3. Top-level `permissions` becomes `{}`: every generated job already sets its own.
4. Every generated job sets `timeout-minutes`: `changes` and the aggregate job get fixed values
   (they run one gate command and a `jq` line), and `[ci.github] jobs.<id>.timeout-minutes`
   becomes required. A breaking configuration change, in the breaking release; the error names the
   job and says to set it.
5. Version comments: `checkout` and a step's `uses` in `[ci.github]` are written as in a workflow,
   `owner/action@<sha> # v4.2.2`. The generator writes the part after ` # ` as a YAML comment after
   the plain scalar (`yaml.rs` gains that one case); the string before it is still checked as a
   commit pin. Today the version sits in a TOML comment beside the string and is lost.
6. The guide's `[ci.github]` example uses `ubuntu-24.04`, not `ubuntu-latest`.
7. `SYNC_STATE_VERSION` 7: a workflow written before is drift.

The generator keeps what it already does that the standard asks: per-job permissions, the
concurrency of T-043, `persist-credentials: false`, the secret in the steps that run `bin/gate`,
a versioned default runner.

## Tests

For each new rule, in `plugins/ci/tests/ci/<rule>.rs`, on synthetic workflows:

- a workflow that meets the rule passes with no finding;
- each aspect's failing case is reported once, at its line, with its aspect and target;
- each option changes the outcome it names (`max-minutes`, `moving`, `version-comment`, `actions`,
  `scopes`);
- the edges the rule pages state: a reusable-workflow call job needs no `timeout-minutes`; a
  `runs-on` expression passes; a matrix name with an expression is not a duplicate; a literal
  `cancel-in-progress: true` passes in a workflow that runs only on `pull_request`.

`ci/actions-pinned`: the existing tests keep their fingerprints (a finding without a version
comment and a tag pin are two findings, the old one unchanged).

Preset: `ci/recommended` turns on every rule at `error` (one test over a workflow that breaks each
rule once).

Generator, in `crates/gate/tests/cli.rs`:

- `the_generated_workflow_meets_the_ci_standard`: `gate sync` from a `[ci.github]` with every key
  (an aggregate, `projects`, `services`, `setup`, `after`, a version comment on `checkout`), then
  `gate check` with `extends = ["ci/recommended"]` and the built `ci` plugin: exit 0, no finding.
- `a_ci_job_without_timeout_minutes_is_a_configuration_error`: exit 2, the message names the job.
- The checkout comment is written after the scalar and the workflow still parses
  (`actionlint` on the round's generated workflow, as T-043's round does).

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny, the
  release build and actionlint exit 0, and the leak scan prints nothing.
- Every new rule has its page; the guide and `CHANGELOG.md` (Unreleased, breaking:
  `timeout-minutes` required) say what changed.
- Do not edit docs/plan.md.

## Open questions for the lead

1. "Packs alike, one name per job": the reading above (one preset taken the same way everywhere;
   one name per job, the aggregate named by its id) is the writer's. Confirm or correct.
2. `timeout-minutes` required in `[ci.github]` (breaking) or a default the generator writes
   (not breaking, but a value nobody chose). The design says required.
3. `ci/environment`'s default `scopes` include `contents`: a release job that pushes a tag then
   needs an environment. Keep, or leave `contents` out and rely on an exception?
4. `ci/node-setup` in this plugin, or left out of v0.3.0.
5. Dependabot configuration is outside workflows and outside this task.

## Resume point

2026-11-19T23:34:40+00:00: survey done (read-only), divergence table written outside the
repository, this design written on `task/T-067-ci-standard` from `main` at `ec66737`. No code yet;
the code starts when the lead approves the design.

# T-067: A standard for CI workflows, checked by the ci plugin

| Field | Value |
| --- | --- |
| Status | done (merged at `de88866`) |
| Release | v0.3.0 |
| Writer | "Gate 006" |
| Reviewer | the lead |
| Depends on | `main` at `340cf8b` |
| Touches | `plugins/ci/src/*` (new rules, the `recommended` preset), `plugins/ci/rules/*.md`, `plugins/ci/tests/ci/*`, `plugins/ci/Cargo.toml` (0.3.0), `plugins/ci/CHANGELOG.md`, `crates/gate-host/src/sync/github.rs`, `crates/gate-host/src/sync/yaml.rs`, `crates/gate-host/src/config/file.rs` (`CiJob`), `crates/gate-host/src/plugins.rs` (`SYNC_STATE_VERSION` only), `crates/gate-host/src/config/mod.rs` (`check_github`), `crates/gate/tests/cli.rs`, `crates/gate/tests/plugins.rs`, `crates/gate-host/tests/host/sync.rs`, `.github/workflows/*.yml`, `docs/guide/*.md`, `README.md`, `docs/architecture.md` section 10, `CHANGELOG.md` |

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

The lead confirmed this reading (see Decisions).

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

Generator:

- `the_generated_workflow_meets_the_ci_standard` (`crates/gate/tests/plugins.rs`, where the built
  plugin components are): `gate sync` from a `[ci.github]` with an aggregate, `projects`,
  `services`, `setup`, `after`, an `environment` and a version comment on `checkout`, then
  `gate check` with `extends = ["ci/recommended"]` and the built `ci` plugin: exit 0. A second,
  hand-written workflow that breaks five rules then makes the check exit 1 with those rules
  reported and `gate.yml` not reported.
- `a_ci_job_without_timeout_minutes_or_with_an_empty_release_is_a_configuration_error`
  (`crates/gate/tests/cli.rs`): exit 2, the message names the job.
- The checkout comment is written after the scalar and the workflow still parses
  (`actionlint` on the round's generated workflow, as T-043's round does).

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny, the
  release build and actionlint exit 0, and the leak scan prints nothing.
- Every new rule has its page; the guide and `CHANGELOG.md` (Unreleased, breaking:
  `timeout-minutes` required) say what changed.
- Do not edit docs/plan.md.

## Decisions (the lead, on design `1881985`)

1. "Packs alike, one name per job": the reading above is confirmed. One preset taken the same way
   everywhere; one name per job; the aggregate job named by its id.
2. `timeout-minutes` is required in `[ci.github]`. The Breaking entry of `CHANGELOG.md` gives the
   migration step (add `timeout-minutes` to each `[ci.github.jobs.<id>]`).
3. `contents` stays in `ci/environment`'s default `scopes`; a repository that wants otherwise
   writes an exception.
4. `ci/node-setup` is in v0.3.0, in `ci/recommended`.
5. Dependabot stays out.
6. Gate's own `.github/workflows` pass `ci/recommended`, the release workflows included, without
   changing their triggers (a release starts only on a tag push that is not batched).

Made while writing the code:

- `[ci.github]` step names stay optional; a step without one gets a `ci/names#step` finding, which
  is how a repository learns to add it. Only the generator's own checkout step is named by Gate.
- `CiJob` gains `environment` (a string): without it a generated job that holds `contents: write`
  could not meet `ci/environment`. The generator writes it after the job's `permissions`.
- `ci/environment` skips a job that calls a reusable workflow (`uses:`): such a job cannot set
  `environment`; the called workflow's jobs are checked in their own file.
- `ci/permissions` and `ci/timeout`: a reusable-workflow call job can set `permissions`, so it is
  asked to; it cannot set `timeout-minutes`, so it is not.

## Gate's own workflows

`gate_s_own_workflows_pass_every_rule` (`plugins/ci/tests/ci/main.rs`) runs every rule of
`ci/recommended` over the repository's `.github/workflows` and asserts no finding. The findings
they had at `340cf8b`, read from the rule pages against the files, and the change made for each:

| File | Finding | Change |
| --- | --- | --- |
| `release.yml` | `ci/permissions#job-missing` on `version` | `permissions: { contents: read }` |
| `release.yml` | `ci/timeout#missing` on `version` and `release` | `timeout-minutes` 15 and 20 |
| `release.yml` | `ci/environment` on `release` (`contents: write`) | `environment: release` |
| `release-plugin.yml` | `ci/names#file`: `Release a plugin` is `release-a-plugin`, not the file's stem | name `Release plugin` |
| `release-plugin.yml` | `ci/permissions#job-missing` on `plugin` | `permissions: { contents: read }` |
| `release-plugin.yml` | `ci/timeout#missing` on `plugin` and `release` | `timeout-minutes` 15 and 20 |
| `release-plugin.yml` | `ci/environment` on `release` | `environment: release` |
| `reproducible.yml` | `ci/names#file`: `Reproducible components` is not the file's stem | name `Reproducible` |
| `reproducible.yml` | `ci/permissions#job-missing` on `build` | `permissions: { contents: read }` |

Not a finding, changed for the standard's item 2: `reproducible.yml` cancelled a running build on
any branch push, `main` included. `cancel-in-progress` is now
`${{ startsWith(github.ref, 'refs/heads/task/') }}`: a task branch's newer push cancels its
running build, a push to `main` and a release's build (a tag ref) run to their end.

`gate.yml`, which `main` gained with T-074, is written by `gate sync` of the core this repository
pins (0.2.1), which predates the standard, so `gate_s_own_workflows_pass_every_rule` leaves out a
workflow that starts with `gate sync`'s marker; this tree's generator is held to the standard by
`the_generated_workflow_meets_the_ci_standard`. Read from the rule pages against the file, the
pinned core's `gate.yml` would get:

| Finding | Where |
| --- | --- |
| `ci/actions-pinned#version-comment` | the two checkout steps (`changes`, `gate`): no release comment |
| `ci/timeout#missing` | `changes`, `gate` and `gate-passed` |
| `ci/names#step` | the two checkout steps: no name |
| `ci/names#job` | `gate-passed`: no name |

The fix, when this repository pins the 0.3.0 core and the ci plugin built for it: in `gate.toml`,
`[gate] version = "0.3.0"`, `timeout-minutes` on `[ci.github.jobs.gate]` (required from 0.3.0),
and the checkout's release inside the string (`"actions/checkout@<sha> # v7.0.1"`, not a TOML
comment); then `gate lock` and `gate sync`. The generator then writes every name, timeout and
release comment above. 0.2.1 cannot express them, so this task leaves `gate.toml` and `gate.yml`
as `main` has them.

The triggers of all three hand-written files are unchanged. The environment `release` is created by GitHub on
the first run that names it, with no protection rules, so a release starts as before; the owner can
add reviewers or a tag policy to it later.

## Known limits

Found by the review of `73d652a`; each errs the way named, and none is fixed in this task.

- Flow style: the release comment is read right after the reference on its line, so a step written
  `- { name: Check out, uses: <action>@<sha> } # v4.2.2` has its comment after the `}` and gets a
  `version-comment` finding.
- Action names are compared as written: `Actions/Checkout@<sha>` is not `actions/checkout` to
  `ci/checkout-credentials` or `ci/node-setup`, though GitHub resolves it.
- Booleans are compared as the lower-case scalar: `persist-credentials: False` is reported as kept
  credentials, and `cancel-in-progress: True` is not reported.
- `ci/concurrency` reads the top-level `concurrency` only: a workflow whose jobs each set their own
  group is reported `missing`.
- `ci/runner-pinned` does not read a matrix: `runs-on: ${{ matrix.os }}` passes whatever the matrix
  lists, `ubuntu-latest` included.
- `ci/timeout` counts a key as set: `timeout-minutes:` with no value (null) passes.

## Resume point

2026-11-20T09:27:42+00:00: code on `task/T-067-ci-standard`, rebased on `main` at `340cf8b`.
- Ticket 328 on `dad51f9`: fmt exit 0; clippy exit 101 on `semicolon_if_nothing_returned` in
  `crates/gate-host/src/sync/yaml.rs` (every other crate it reached, `gate-plugin-ci` among them,
  checked clean from this worktree); fixed in `ee27683`.
- Ticket 333 on `ee27683`, target `T-067/target`: fmt, clippy, the ci plugin tests (31), the host
  yaml test, the two new `gate` tests by name, `cargo test --workspace --no-fail-fast` (1093
  passed, 0 failed, 7 ignored), deny, the release build, actionlint on Gate's own workflows and
  actionlint on a workflow the release build's `gate sync` wrote (version comments after each
  pin) exit 0; `done in 797s with exit code 0`. Its logs show `gate`, `gate-host` and
  `gate-plugin-ci` compiled from this worktree, and all 27 new tests listed by name.
- 2026-11-20T16:33:45+00:00: rebased onto `main` at `cee6491` (batches 1 and 2). `main` moved the YAML reader to
  `gate_sdk::yaml` (feature `yaml`), so every rule imports `gate_sdk::yaml::Node`; conflicts in
  `plugins/ci/src/actions_pinned.rs` (imports), `crates/gate/tests/plugins.rs` (both sides add
  tests at the end; kept both, with this branch's `timeout-minutes` edits) and
  `docs/guide/plugins.md` (the presets paragraph names `ci/recommended` beside the new plugins'
  presets). The `CHANGELOG.md` entries move from the released 0.2.1 section into 0.3.0. No
  `[ci.github]` job that `main` added lacks `timeout-minutes`; Gate's workflows are unchanged on
  `main`. Not yet run in the lane on this tip.
- 2026-11-20T17:02:08+00:00: ticket 348 on `6edc90c`, target `T-067/target`: fmt, clippy, the ci plugin tests, the
  host yaml test, the two new `gate` tests by name, `cargo test --workspace --no-fail-fast` (1370
  passed, 0 failed, 7 ignored), the release build, actionlint on Gate's own workflows and on a
  generated one exit 0; `done in 1128s`. `cargo deny` exit 1 on RUSTSEC-2026-0587 alone
  (`wasmtime-wasi` 50.0.2 in `main`'s lock, Windows only; T-073 bumps it), which the lead does
  not count against the round. Its logs show `gate`, `gate-host`, `gate-sdk` and
  `gate-plugin-ci` compiled from this worktree and the 27 new tests listed by name.
- 2026-11-20T17:58:39+00:00: rebased onto `main` at `7f62554b` (T-073, wasmtime 50.0.3; T-074, Gate checks its own
  repository) without conflict. `gate_s_own_workflows_pass_every_rule` now leaves out the
  workflow the pinned core generates (`gate.yml`, new on `main`); its findings and the fix at the
  0.3.0 pin are listed under "Gate's own workflows". Not yet run in the lane on this tip.
- 2026-11-20T18:10:00+00:00: the review of `73d652a` (one fix round): `ci/concurrency` leaves out `missing` for
  a workflow whose only trigger is `workflow_call`; `ci/node-setup` passes a `node-version` that is
  an expression; `ci/secrets-in-steps` reads a value as a secret only when it is an expression; the
  generated YAML writes a `uses` release comment only in a job or a step; the guide gives a
  monorepo root the `[rules]` form (`"ci/*" = "error"`, tested by
  `a_monorepo_root_takes_the_ci_standard_through_its_rules`) and lists every finding the preset
  still reports on a generated workflow; `version-comment` is a breaking change of ci 0.3.0, the
  version of `plugins/ci/Cargo.toml`; the remaining edges are under "Known limits". Not yet run in
  the lane on this tip.
- 2026-11-20T20:10:46+00:00: ticket 370 on `0d3a25c`, target `T-067/target`: fmt, clippy, the ci plugin tests
  (34), the host yaml test, the cli test and the two `gate` plugins tests by name, `cargo test
  --workspace --no-fail-fast` (1374 passed, 0 failed, 7 ignored), deny, the release build,
  actionlint on Gate's own workflows and on a workflow the release build's `gate sync` wrote exit
  0; `done in 618s with exit code 0`. Its logs show `gate`, `gate-host`, `gate-sdk` and
  `gate-plugin-ci` (v0.3.0) compiled from this worktree, none from another, and all 31 new tests
  listed by name. The reviewer accepted `0d3a25c`.
- Next: the lead merges.

# T-069: Every shared plugin ships two presets

| Field | Value |
| --- | --- |
| Status | rebased onto `main` at `d0c6cda3`; round 385 green; merges last of 0.3.0 |
| Release | v0.3.0 (presets only; no interface change) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | `main` at `340cf8b` |
| Touches | the `presets` of `plugins/{agents,ci,git,ledger,markdown,nestjs,typescript}/src/lib.rs`, one preset test per plugin, one CLI test in `crates/gate/tests/plugins.rs`, `docs/guide/plugins.md`, the root `CHANGELOG.md` |

## Goal

For 0.3.0 each shared plugin has two presets, built like `git/recommended`:

- `<plugin>/recommended`: every rule the preset may turn on, at `error`;
- `<plugin>/adoption`: the same rules at `warning`, for a repository on its way to the standard.

On `main` only `git` ships a preset (`git/recommended`), so this adds `git/adoption` and both
presets of `agents`, `ci`, `ledger`, `markdown`, `nestjs` and `typescript`. `node` already has both
on its own branch (T-065).

## Which rules a preset leaves out

A preset never turns on a rule that needs something only the project can give. Read from `main` at
`340cf8b` (the rule's options type, its `NEEDS`, and its page's options table), that is a rule:

1. with an option that has no default, where the options without it are a configuration error;
2. that `NEEDS` a plugin option with no default;
3. that is off, or reports nothing, until the project sets an option with no meaningful default (an
   empty string or list that turns it off);
4. whose main option is the project's own inventory or registry, which is empty by default and
   which the rule holds every file to (so an empty one reports every controller or field, or
   checks nothing).

One plugin setting is the exception to reason 2, by the lead's ruling after round 380: every
`frontend` rule needs `pack`, the plugin's one required setting, which names the project's framework
(`react-vite`, `nextjs` or `react-library`). No preset can choose it, so the `frontend` presets keep
every rule and a project that extends either one sets `[plugins.frontend] options pack` first. No
other plugin gets this exception.

Each preset names the same rules. The two differ only in the level. A rule a preset leaves out is
listed on the plugin's section of `docs/guide/plugins.md` with its reason, and is turned on in
`[rules]` with its options.

### `agents` (9 rules): all 9 in

`instruction-references`, `no-claude-md`, `rule-enforcement`, `rule-file-size`, `rule-ids`,
`rule-owner`, `rule-references`, `rule-registry`, `skills`. Every option has a default, and a
rule without its files reports nothing. The aspect of `instruction-references` that reads
`command-pattern` stays off until that option is set, and the rule's other aspects run without it.
The generators `adapters` and `agents-block` are not rules and no preset names them.

### `ci` (2 rules): both in

`actions-pinned`, `workflow-run-target`. `workflows` defaults to `.github/workflows/*.{yml,yaml}`.

### `git` (4 rules): 2 in, 2 out

- In: `branch-name` with `template = ":type/:ticket-:name"` (its options need a `template` or a
  `pattern`, and `git/recommended` gives the template), `commit-message` (in `recommended` with the
  two aspect levels `git/recommended` gives today, `header-min-length` and `scope-max-length` at
  `warning`).
- Out: `main-history` and `no-ci-skip`, which `NEEDS` plugin options only the repository can give
  (reason 2). Both need `activation` (the commit the policy starts after) and `release-subject`;
  `main-history` also needs `branch-pattern`, `branch-length`, `dependabot` and `documentation`.

`git/recommended` keeps its content and gains an explicit `"level": "error"` per rule. `git/adoption`
gives both rules `warning` with the same template.

### `ledger` (9 rules): 6 in, 3 out

- In: `blockers`, `decisions`, `exception-decisions`, `exception-owners`, `manifest`, `records`.
  Each reads a path with a default and reports nothing while that file is not tracked.
- Out:
  - `closing-notes`: off until `field` is set; its default `""` turns it off (reason 3).
  - `reuses`: off until `catalog` is set (reason 3).
  - `record-status`: reports nothing without the plugin option `parity`, which has no default
    (reasons 2 and 3).

T-061 makes these three options required, which also gives them reason 1.

### `markdown` (3 rules): 2 in, 1 out

- In: `frontmatter`, `local-links`.
- Out: `required-sections`. Its `sections` (which page needs which headings) defaults to `[]`, and
  then the rule checks nothing (reason 3).

### `nestjs` (37 rules): 25 in, 12 out

- In: `adapter-boundary`, `api-readiness`, `automator-metadata`, `barrel-exports`,
  `constant-files`, `custom-routes`, `declaration-files`, `dto-decorators`, `entity-descriptions`,
  `entity-table-name`, `enum-values`, `error-logging`, `error-string-shim`, `eslint-directives`,
  `import-aliases`, `kind-layout`, `migrations`, `module-names`, `module-registry`,
  `native-functions`, `native-repository`, `prefix-nesting`, `repeated-segment`,
  `typeorm-relation`, `wrappers`. `string-enums` left for `typescript` at the rebase (T-064), and
  `error-string-shim`, which T-064 split out of it, is in: its `enum-names` defaults to the CRUD
  automator's two enums, and it needs no plugin option.
- Out, an option without a default (reason 1); the options are an error without it:
  - `advisory-lock-boundary` (`executor`);
  - `import-direction` (`contracts`);
  - `notification-egress` (`owner-classes`, `egress.modules`, `owners`);
  - `owner-boundaries` (`owner-roots`, `api-owners`, `domain-owner`, `internal-owner`,
    `plane-routes`, `internal-base`);
  - `owner-roots` (`required-roots`);
  - `raw-config-access` (`raw-imports`);
  - `runtime-factory` (`contracts`).
- Out, the project's own inventory (reason 4):
  - `controller-owners`: every controller must have a row of `controllers`, default `[]`;
  - `field-inventory`: every free-text field must have an entry of `fields`, default `[]`;
  - `package-pins`: checks the packages of `artifacts`, default `[]`; with none it only reports a
    missing `package-lock.json`.
- Out, a subsystem a service may not have (reason 3, the lead's ruling):
  - `advisory-lock-manifest`: only a service that uses advisory locks has the manifest;
  - `asset-storage`: only a service that stores assets has the storage adapter.

`api-readiness` is in: every service is readiness-checkable, which is part of the NestJS standard
(the lead's ruling).

### `typescript` (3 rules): all in

`parse`, `barrel-imports`, accepted by the lead in the review. `owners` and `exempt` default to
`[]`, and the rule still checks every folder that has a barrel. `string-enums`, moved from `nestjs`
by T-064: every option has a default (`test-roots` among them), and it needs no plugin option.

### `cdk`, `docker`, `docs`, `node` and `repository`: already two presets

These plugins came to `main` with both presets, every rule in each. `cdk/adoption` warned only on
the renames and kept its other rules at `error`; at the rebase it turns on every rule at `warning`,
as the ruling of this task has it (the lead, 2026-11-20). `repository/recommended` keeps
`repository/large-files#large` at `warning`, an aspect level, as `git/recommended` keeps two.

## The rebase onto `main`

The lists were written against `main` at `340cf8b`; the branch is rebased onto `16d94316` (after
T-064, T-061, T-062, T-066, T-066b, T-073 and T-074):

- `nestjs/string-enums` is `typescript/string-enums` (T-064): it leaves the `nestjs` presets and
  joins the `typescript` ones. `nestjs/error-string-shim` joins the `nestjs` presets.
- T-061 and T-062 added no rule to `agents` or `ledger` (each still declares nine), so their
  presets stand as written; the preset tests' load check holds, in the round, that no included
  rule's option became required.
- `cdk/adoption` turns on every rule at `warning` (above).
- `cdk`, `docker`, `docs`, `node` and `repository` each gain the load check of the preset tests (a
  `presets` test module). Their own tests already hold that each preset names every rule at its
  level; `cdk`'s now holds `adoption` at `warning` for every rule.

A second rebase, onto `d0c6cda3` (after T-044, T-067, T-068, T-071 and T-082, among others):

- T-067 grew `ci` to eleven rules and gave it `ci/recommended`, every rule at `error`. `ci/adoption`
  joins it with the same eleven at `warning`; T-067's own test now expects both presets, and the
  `ci` preset test of this task holds both with no rule left out.
- `frontend` came with both presets, its twenty rules in each, and no preset test: it gains this
  task's preset test (both presets, their levels, and the load check).
- T-082 wrote each plugin's 0.3.0 changelog section; the seven plugins whose presets this task
  adds or changes (`agents`, `ci`, `git`, `ledger`, `markdown`, `nestjs`, `typescript`) gain a
  Features entry there.
- The plugins guide's Presets paragraph keeps T-071's places (`[project]`, `[workspace]` at a root
  that is not a project, a profile) and says every shared plugin ships two; the table of shared
  plugins lists both presets of each.

## Tests

- One test per plugin of this task, in its tests: both presets exist and name exactly the
  plugin's rules minus the excluded ones, each rule with `level` `error` in `recommended` and
  `warning` in `adoption`.
  The exclusions are a list in the test with each reason, so a new rule fails the test until it is
  added to the presets or to the list.
- One CLI test in `crates/gate/tests/plugins.rs`: a repository with `extends = ["ci/adoption"]` and
  an action pinned by tag reports `ci/actions-pinned` at `warning`, and exits 0.
- `git`'s test also holds the exact settings of both presets: the branch template in each, and
  `recommended`'s two aspect levels.
- Each preset test also holds that every rule a preset turns on loads with what the preset gives
  it: its options schema requires no key the preset leaves out (and one `oneOf` alternative is
  given, as `git/branch-name`'s `template`), and its `x-gate-needs` has no entry without a `when`.
  So a branch that makes an included rule's option required fails here, not in a project that
  extends the preset (the review's first finding).
- `frontend`'s check accepts a need of `pack` only: it runs every preset rule once per pack with
  that pack given, through the SDK on a synthetic tree, and every run must succeed; without a pack,
  every preset rule must stop with the existing error, whose text the test asserts (it names `pack`,
  the three packs and `[plugins.frontend] options`).

## Docs

- `docs/guide/plugins.md`: each plugin's section lists its two presets and the rules they leave out
  with the reason. The Presets section no longer says that no shared plugin ships one.
- Root `CHANGELOG.md`: one Features entry under a `# 0.3.0 (unreleased)` section of its own, the
  shape of the other 0.3.0 branches. `## Unreleased` on `main` becomes `## 0.2.1` at that release,
  so an entry there would land in the wrong one (the lead's ruling); the lead merges the 0.3.0
  headings at the cut. The plugins' own changelogs are left to the cut too.

## Done when

- The lead accepts the lists above.
- One lane round on the pushed tip, with its own target directory and its logs kept per ticket:
  fmt, clippy, test, deny and the release build exit 0. The log shows each changed crate compiled
  from this task's round checkout and lists each new test by name. Each new preset test fails with
  a rule taken out of its preset.

## The independent review of `0e07734`, one fix round

Nothing blocking. Four minor findings, each fixed:

1. Each preset test also holds that every rule a preset turns on loads with what the preset gives
   it (see Tests). Proved by a mutation that makes `ci`'s `workflows` option required.
2. The plugins guide's `typescript` table lists `typescript/barrel-imports`, and its presets name
   both rules; the lead accepted `parse` and `barrel-imports` as the `typescript` presets.
3. The guide, `git/recommended`'s comment and the `git` test name every plugin option
   `main-history` needs, not only `activation` and `release-subject`.
4. The changelog entry says "an option without a default or whose default turns the rule off" and
   names the subsystem reason (`nestjs/advisory-lock-manifest`, `nestjs/asset-storage`).

The `typescript` section of this file, lost from `bb9cf53` by an edit of the `nestjs` section, is
back.

## Proof

Lane ticket 322 on `bb9cf53`, 2026-11-20T07:21:59+00:00 to 2026-11-20T07:45:29+00:00
(`round-inner.sh`, its own target directory, logs kept under the ticket):

| Step | Command | Exit |
| --- | --- | --- |
| fmt | `cargo fmt --all --check` | 0 |
| clippy | `cargo clippy --offline --locked --workspace --all-targets -j 4 -- -D warnings` | 0 |
| test | `cargo test --offline --locked --workspace -j 4 --no-fail-fast` | 0, 1074 passed, 0 failed |
| deny | `cargo deny --offline --locked check` (advisories, bans, licenses, sources ok) | 0 |
| release build | `cargo build --release --offline --locked -j 4` | 0 |

The test log shows `Compiling` for `gate-plugin-agents`, `gate-plugin-ci`, `gate-plugin-git`,
`gate-plugin-ledger`, `gate-plugin-markdown`, `gate-plugin-nestjs`, `gate-plugin-typescript` and
`gate` from the round's own checkout of `bb9cf53`, and lists by name, each `ok`, the seven preset
tests (`both_presets_turn_on_every_rule_but_the_ones_left_out` in each plugin) and
`a_project_that_extends_an_adoption_preset_is_told_at_warning_and_passes`.

Each mutation, applied to the round's tree, made its exact test fail (exit 101):

| Mutation | Test |
| --- | --- |
| `agents`: `rule-ids` taken out of `recommended` | `agents` `presets::both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `ci`: `actions-pinned` taken out of `recommended` | `ci` `presets::both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `git`: the branch template taken out of `adoption` | `git` `presets::both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `ledger`: `manifest` taken out of `adoption` | `ledger` `presets::both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `markdown`: `frontmatter` taken out of `recommended` | `markdown` `presets::both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `nestjs`: `api-readiness` taken out of `adoption` | `nestjs` `both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `typescript`: `parse` taken out of `adoption` | `typescript` `both_presets_turn_on_every_rule_but_the_ones_left_out` |
| `ci/adoption` gives `actions-pinned` `error` | `gate` `a_project_that_extends_an_adoption_preset_is_told_at_warning_and_passes` |

### Proof of the fix round

Lane ticket 332 on `37b05a1`, 2026-11-20T09:02:43+00:00 to 2026-11-20T09:10:47+00:00
(`round-inner.sh`, its own target directory, logs kept under the ticket): fmt 0, clippy 0, test 0
(1074 passed, 0 failed), deny 0 (advisories, bans, licenses, sources ok), release build 0. The test
log shows `Compiling` for the seven plugins and `gate` from the round's own checkout of `37b05a1`,
and lists the seven preset tests and the CLI test by name, each `ok`. The nine mutations each made
their exact test fail (exit 101): the eight of round 322, and `ci`'s options without their default
(so `workflows` is required), which `ci`'s preset test catches through the new load check.

### Proof of the round after the second rebase

Lane ticket 385 on `4723f9ae`, 2026-11-20T22:39:46+00:00 to 2026-11-20T22:53:37+00:00
(`round-inner.sh`, its own target directory, logs kept under the ticket): fmt 0, clippy 0, test 0
(1459 passed, 0 failed), deny 0 (advisories, bans, licenses, sources ok), release build 0. The test
log shows `Compiling` for every `gate` crate and plugin from the round's own checkout of
`4723f9ae`, and lists each plugin's preset test, the five load checks, `cdk`'s
`both_presets_set_every_rule_at_their_one_level` and the CLI test by name, each `ok`. Ticket 380 on
`0fcdc642` had failed `frontend`'s preset test, because every `frontend` rule needs `pack`; by the
lead's ruling the check now gives each preset rule each pack in turn. All fifteen mutations made
their exact test fail (exit 101); the last, `file-names` taken out of `frontend/adoption`, fails
`frontend`'s test on the list of rules (`adoption`), no longer on `pack`.

## Resume point

2026-11-20T22:54:29+00:00: ticket 385 on `4723f9ae` is green with every mutation caught. The branch
merges last of 0.3.0; if `main` moves in code it touches, it rebases and runs one more round.

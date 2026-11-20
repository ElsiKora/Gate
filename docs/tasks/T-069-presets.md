# T-069: Every shared plugin ships two presets

| Field | Value |
| --- | --- |
| Status | design; the lists below wait for the lead before any code |
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
- Out: `main-history` and `no-ci-skip`. Both `NEEDS` `activation` (the commit the policy starts
  after) and `release-subject`, which only the repository can give (reason 2).

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

### `nestjs` (37 rules): 24 in, 13 out

- In: `adapter-boundary`, `automator-metadata`, `barrel-exports`, `constant-files`, `custom-routes`,
  `declaration-files`, `dto-decorators`, `entity-descriptions`, `entity-table-name`, `enum-values`,
  `error-logging`, `eslint-directives`, `import-aliases`, `kind-layout`, `migrations`,
  `module-names`, `module-registry`, `native-functions`, `native-repository`, `prefix-nesting`,
  `repeated-segment`, `string-enums`, `typeorm-relation`, `wrappers`.
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
- Out, a subsystem a service may not have (my proposal for the lead's ruling): each reports a
  service that does not have the subsystem at all.
  - `advisory-lock-manifest`: reports a missing `registry.manifest.json`.
  - `asset-storage`: reports a missing S3 adapter (`#s3-adapter-required`).
  - `api-readiness`: reports each missing readiness file and a missing startup file.

  If the lead rules these are part of the standard every service follows, they move in.

### `typescript` (2 rules): both in

`parse`, `barrel-imports`. `owners` and `exempt` default to `[]`, and the rule still checks every
folder that has a barrel.

## Moves on other branches

The lists are written against `main` at `340cf8b`. This branch changes only the preset arrays, docs
and tests, so the rebase stays mechanical. It merges last of the 0.3.0 branches and is rebased then:

- T-064 moves `nestjs/string-enums` to `typescript/string-enums`: at the rebase it leaves the
  `nestjs` presets and joins the `typescript` ones (its options keep their defaults). T-064 also adds
  `nestjs/error-string-shim`, which gets a decision at the rebase, and the preset test fails until
  it has one.
- T-061 and T-062 change `agents` and `ledger`. A rule they add fails the preset test until it has a
  decision. An option they make required moves its rule out, with reason 1.

## Tests

- One test per plugin, in its tests: both presets exist and name exactly the plugin's rules minus
  the excluded ones, each rule with `level` `error` in `recommended` and `warning` in `adoption`.
  The exclusions are a list in the test with each reason, so a new rule fails the test until it is
  added to the presets or to the list.
- One CLI test in `crates/gate/tests/plugins.rs`: a repository with `extends = ["ci/adoption"]` and
  an action pinned by tag reports `ci/actions-pinned` at `warning`, and exits 0.

## Docs

- `docs/guide/plugins.md`: each plugin's section lists its two presets and the rules they leave out
  with the reason. The Presets section no longer says that no shared plugin ships one.
- Root `CHANGELOG.md`: one Features entry. `main` has `## Unreleased`, not `# 0.3.0 (unreleased)`,
  so the entry goes under `## Unreleased`, `### Features`, and the lead places the heading at the
  cut.

## Done when

- The lead accepts the lists above.
- One lane round on the pushed tip, with its own target directory and its logs kept per ticket:
  fmt, clippy, test, deny and the release build exit 0. The log shows each changed crate compiled
  from this task's round checkout and lists each new test by name. Each new preset test fails with
  a rule taken out of its preset.

## Resume point

2026-11-20T05:23:02+00:00: the lists are written and sent to the lead. No code until the lead
answers.

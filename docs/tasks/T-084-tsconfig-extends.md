# T-084: A tsconfig base outside the project

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 (blocker 1 of the release-candidate trial, T-083) |
| Writer | Gate 001 |
| Reviewer | a session that did not write it |
| Depends on | `main` at `30a1da26` |
| Touches | `languages/typescript/src/{resolver,call,lib}.rs`, `languages/typescript/Cargo.toml`, `Cargo.toml`, `Cargo.lock`, `crates/gate-sdk/src/rule.rs`, `plugins/cdk/src/sources.rs`, `plugins/typescript/src/{lib,parse,findings,barrel_imports}.rs`, `plugins/typescript/src/string_enums/mod.rs`, tests of `languages/typescript`, `nestjs`, `cdk` and `frontend`, `docs/guide/{configuration,monorepo,plugins}.md`, the `cdk`, `frontend` and `node/tsconfig-base` rule pages, `CHANGELOG.md`, the `nestjs` and `typescript` changelogs |

## Goal

In a monorepo, a project's `tsconfig.json` with `"extends": "../../tsconfig.base.json"`, the
layout `node/tsconfig` requires, stops every `nestjs`, `cdk` and `frontend` rule that resolves an
import with an environment error, and `gate check` exits 2 with no finding (T-083, finding 1). The
module resolver of the TypeScript language library sees only the project's tracked files, so a
base above the project root is never found, and `reads` on the plugin changes nothing because the
resolver never asks the tree for it.

## Change

- The resolver reads the `extends` chain of the configured tsconfig through the plugin's tree
  before it starts: each relative base is read by its project-relative path, `..` included, so a
  base outside the project is read when the plugin's `reads` grants it, never from the host's file
  system. The virtual file system is mounted deep enough for every `..` of the chain, and error
  texts name paths relative to the project root (`../tsconfig.base.json`, not
  `/gate/tsconfig.base.json`).
- A base that cannot be read (a relative one not tracked, not granted or outside the repository,
  an absolute one, an empty entry, or a `#` entry whose `package.json` `imports` target cannot be
  read) is dropped from the `extends` the resolver reads: the tsconfig is rewritten without it
  (comments and trailing commas stripped with `json-strip-comments`, already in the lock through
  oxc), so resolution goes on without its `paths`. The call reports it once, as a finding of the
  first rule that resolved an import, at the tsconfig that names the base, naming the file the
  compiler would read (`tsconfig.local.json`, `dir/tsconfig.json`) and what to do, with the
  `reads` grant for a base outside the project; the help says that other findings about imports
  may come from the missing base. A base from an untracked package (`@tsconfig/node22`), or a `#`
  entry that maps to one, is dropped without a finding, as an import of an untracked package
  resolves to nothing; so is a tracked workspace package reached only through an untracked
  `node_modules` link, which the configuration guide names with the relative-path way out.
- The resolver mounts the repository and the project at its real root, so a base's `paths`
  written from the repository root (`apps/api/src/*`) lead back into the project; the view learns
  the project from the call (`TypeScript::in_project`).
- The SDK's `CallState` gains `report`, which a call state uses to report what it found while a
  rule used it; the default reports nothing. `TypeScriptCall` (nestjs, frontend) and the `cdk`
  call state report through `report_unreadable_bases`. The `typescript` plugin moves to
  `TypeScriptCall`, so its two resolving rules share one view and the call reports once.

## Tests

- `languages/typescript/tests/typescript.rs`:
  `a_tsconfig_base_above_the_project_is_read_through_the_source_and_its_paths_apply`,
  `a_tsconfig_base_the_source_cannot_read_is_left_out_and_listed`,
  `every_kind_of_unreadable_base_is_left_out_and_untracked_packages_are_not_listed` (relative,
  outside the repository, absolute, extension-less, directory, empty and `#` bases; untracked
  packages and a `#` entry mapping to one skipped; a `#` entry mapping to a tracked file kept, its
  `paths` applied), `a_directory_base_above_the_project_is_read_as_its_tsconfig_json`,
  `a_resolution_error_names_a_base_above_the_project_relative_to_the_project_root`.
- `plugins/nestjs/tests/tsconfig_base.rs`:
  `a_service_whose_tsconfig_extends_a_granted_base_above_the_project_passes`,
  `a_base_the_plugin_cannot_read_is_a_finding_with_the_grant_to_add`.
- `plugins/cdk/tests/rules/tsconfig_base.rs`:
  `an_app_whose_tsconfig_extends_a_granted_base_above_it_passes_every_rule`,
  `a_base_the_plugin_cannot_read_is_one_finding_of_the_call_with_the_grant_to_add`,
  `the_base_finding_moves_to_the_next_rule_when_the_first_rule_does_not_run`.
- `plugins/frontend/tests/tsconfig_base.rs`:
  `an_app_whose_alias_is_in_a_granted_base_above_the_project_resolves_it`,
  `a_base_the_plugin_cannot_read_is_a_finding_with_the_grant_to_add`.

## Done when

- A monorepo project that extends `../../tsconfig.base.json` with the base granted resolves its
  aliases from the base in `nestjs`, `cdk` and `frontend`, with no environment error.
- Without the grant, the run reports the unreadable base as a finding with the advice and exits
  without an environment error.
- One lane round on the task's tip passes: build, fmt, clippy, test, deny.

## Resume point

2026-11-21T00:29:59+00:00: the review's points are folded in (a `#` base, the help's note on other
findings, one report per call in `typescript`, the `node_modules` link case, the file oxc reads,
the directory base and next-rule tests, the empty entry). Next: one lane round on the tip.

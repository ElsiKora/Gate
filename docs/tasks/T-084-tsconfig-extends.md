# T-084: A tsconfig base outside the project

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 (blocker 1 of the release-candidate trial, T-083) |
| Writer | Gate 001 |
| Reviewer | a session that did not write it |
| Depends on | `main` at `30a1da26` |
| Touches | `languages/typescript/src/{resolver,call,lib}.rs`, `crates/gate-sdk/src/rule.rs`, tests of `languages/typescript`, `nestjs`, `cdk` and `frontend`, `CHANGELOG.md` |

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
- A relative or absolute base that cannot be read (not tracked, not granted, or outside the
  repository) stands as an empty tsconfig, so resolution goes on without its `paths`, and the call
  reports it once, as a finding of the first rule that resolved an import, at the tsconfig that
  names the base, with the `reads` grant to add. A base from a package (`@tsconfig/node22`) stands
  as an empty tsconfig without a finding, as an import of an untracked package resolves to
  nothing.
- The SDK's `CallState` gains `report`, which a call state uses to report what it found while a
  rule used it; the default reports nothing.

## Done when

- A monorepo project that extends `../../tsconfig.base.json` with the base granted resolves its
  aliases from the base in `nestjs`, `cdk` and `frontend`, with no environment error.
- Without the grant, the run reports the unreadable base as a finding with the advice and exits
  without an environment error.
- One lane round on the task's tip passes: build, fmt, clippy, test, deny.

## Resume point

2026-11-21T00:01:19+00:00: task file written on `task/T-084-tsconfig-extends` off `30a1da26`.
Next: the resolver change and its tests.

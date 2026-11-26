# T-107: a nested project's files are not judged by the root project's rules

| Field | Value |
| --- | --- |
| Status | ready for review |
| Release | the next core release or the `typescript` plugin release, whichever holds the cause |
| Writer | "Gate T-107" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `80db512` |
| Touches | where the cause is: the host's split of files between projects, or `plugins/typescript/` (its project index and `typescript/barrel-imports`); tests; `docs/plan.md`, this task file |

## Goal

A repository has two projects:

- the root project runs `typescript/barrel-imports` (from `typescript/recommended`);
- a nested project, in its own folder with its own `gate.toml`, does not run that rule.

With v0.3.2 and typescript 0.3.1, 323 `typescript/barrel-imports` findings on the nested project's
`.ts` files are reported under the root project. `docs/guide/monorepo.md` says a nested project owns
its files. No other root rule reported a path of the nested project in the same run. Seen on the
first consumer.

## Required outcome

1. Find the cause, and say in this file whether it is in the core or in the typescript plugin:
   - the files the core hands the root project's call;
   - or the plugin's own project index, for example a tsconfig or a barrel map that spans the
     whole repository.
2. A finding of a root project's rule is never on a file that a nested project owns.
3. Fix the cause where it is:
   - If it is the plugin, `typescript/barrel-imports` and any other typescript rule built on the
     same index keep to the project's own files.
   - If it is the core, every rule does.
4. Resolving an import into the nested project's files, for example to read its barrel, is still
   allowed where the rule needs it. Only findings and judged files are limited.

## Proof

- A synthetic fixture with a root project and a nested project, each with a barrel and a deep
  import that the rule would report:
  - with the rule on in the root only, there is no finding on the nested project's files and the
    root's own finding stays;
  - with the rule on in both, each finding is reported under its own project.
- No other typescript rule's findings change on the existing fixtures.
- All cargo runs go through the lane.

## Cause

The cause is in the core, so every rule had it.

- The host builds a project call's grant from the project root and the plugin's `reads` only
  (`grant` in `crates/gate-host/src/plugins.rs`).
- `Grant` (`crates/gate-runtime/src/grant.rs`) knows no other project's root. So the root
  project's call (root `""`) lists and reads every tracked path, the nested project's subtree
  included. `apps/server` does the same when `apps/server/tools/cli` is a project.
- The typescript view lists its files from that listing (`CallSource` -> `TypeScript::files`), and
  `typescript/barrel-imports` judges each file listed. Any rule that walks its tree does the same.
  The other root rules on the consumer simply found nothing to report there.
- A plugin cannot correct for this itself. A project from pnpm, npm or a glob has no `gate.toml`
  to spot, and `gate:plugin@0.3.0` does not hand the call the roots of nested projects.

## Fix

The grant is unchanged: a call still lists and reads the nested project's files, so an import that
resolves into them is still followed. For example, the root's deep import into the nested project's
barrel folder is still reported, at the root's own file.

After each project call, the host drops a finding when both hold:

- its file location lies inside the call's project root;
- the path is owned by another project, the one with the longest root that holds it
  (`in_nested_project` in `crates/gate-host/src/plugins.rs`).

These findings stay:

- a finding outside the root (a granted read, a root profile's option file);
- a finding at an option file of one of the call's rules;
- a finding on a commit, or with no location;
- every finding of the repository's call.

The filter runs on the call's outcome, so an answer served from the cache is filtered too.
`gate:plugin@0.3.0` is unchanged, and no plugin changes.

## Lead's conditions (approved 2026-11-26)

- Filter after the call in the core. The grant, the listing and the WIT stay unchanged.
- One owner function, `Projects::owner` (longest root), decides both the filter and project
  assignment.
- Tests for three cases: an unused exception reported through `gate/exceptions`, a cached run equal
  to a cold one, and a project found by glob with no `gate.toml`.
- One sentence each in `docs/guide/monorepo.md` and `docs/architecture.md`.
- A before/after timing of a cold `check`.

## Resume point

- Tests in `crates/gate/tests/plugins.rs`:
  - `a_root_projects_rule_reports_no_file_a_nested_project_owns`: the built typescript plugin, run
    cold, then warm from the result cache with the same findings, then with the rule on in both;
  - `a_root_rules_exception_on_a_nested_projects_file_matches_no_finding`;
  - `a_projects_rule_reads_a_nested_projects_file_but_reports_nothing_on_it`: the runtime probe,
    with the nested project found by its `gate.toml` and by a glob.
- The fix is `in_nested_project` in `crates/gate-host/src/plugins.rs`; the docs are done.
- Red, through the lane, on `21b4518` with the final tests (main's host code), started at
  2026-11-26T02:53:22+00:00. The command was `cargo test --offline --locked -j 4 -p gate --test
  plugins -- nested_project`. It exited 101: the three new tests failed and the three earlier
  nested-project tests passed.
  - The root reported `infra/cdk/bin/app.ts`.
  - The root exception matched, so `gate/exceptions` reported nothing.
  - The probe's finding "on the nested file" was reported.
- The round on `df39cf1`:
  - fmt exited 0;
  - clippy exited 101 (`too_many_lines` 104/100 in `Loaded::run`), fixed in the next commit by
    moving the filter into `own_findings`;
  - the test exited 0 (1656 passed, 0 failed);
  - deny exited 0.
- Timing: `measure.sh 21b45187 df39cf1b` used release builds of `gate` at both commits and the
  release typescript component. The synthetic tree had 12,004 tracked files: 6,000 `.ts` files per
  project, a root project running barrel-imports and a nested project with its own `gate.toml`
  that does not. It ran three alternating cold `check` runs, with results and answers cleared and
  compiled modules kept, at load about 17, ending 2026-11-26T03:13:58+00:00.

  | Commit | Wall time per run | Findings | Under the nested project |
  | --- | --- | --- | --- |
  | base `21b4518` | 0.55 s, 0.45 s, 0.46 s | 3000 | 1500 |
  | head `df39cf1` | 0.51 s, 0.48 s, 0.47 s | 1500 | 0 |

  The filter's cost is within the noise.
- The round on `31a9865`:
  - fmt exited 0;
  - clippy exited 101 (`type_complexity` in the test helper `barrel_check`), fixed in the next
    commit with a type alias;
  - the test exited 0 (1656 passed, 0 failed);
  - deny exited 0.
- The round on `550c4e8` (`round.sh`, a clean checkout, the shared target, through the lane, from
  2026-11-26T07:45:54+00:00 to 08:08:03+00:00):
  - `cargo fmt --all --check` exited 0;
  - `cargo clippy --offline --locked --all-targets -j 4 -- -D warnings` exited 0;
  - `cargo test --offline --locked -j 4` exited 0 (1656 passed, 0 failed);
  - `cargo deny --offline --locked check` exited 0.
  - Later commits change only this task file and `docs/plan.md`.
- The typescript plugin is unchanged, so no typescript rule's findings change on the existing
  fixtures: its tests are in the 1656 that passed.
- Next: the independent review.

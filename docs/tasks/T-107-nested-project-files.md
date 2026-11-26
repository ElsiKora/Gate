# T-107: a nested project's files are not judged by the root project's rules

| Field | Value |
| --- | --- |
| Status | in progress |
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

## Resume point

- Tests:
  - `a_root_projects_rule_reports_no_file_a_nested_project_owns` (built typescript plugin, root
    and nested project);
  - `a_projects_rule_reads_a_nested_projects_file_but_reports_nothing_on_it` (runtime probe: reads
    the nested file, keeps the finding at its option file in the nested folder, drops the finding
    on the nested file).
  - Both are in `crates/gate/tests/plugins.rs`. Their red run on the test-only commit `21b4518`
    is queued in the lane.
- The fix is in `in_nested_project`. Docs: `docs/guide/monorepo.md` and `docs/architecture.md`.
- Next: the red run, then fmt, clippy, the full test and deny through the lane on the head; then
  push and report the head OID to the lead.

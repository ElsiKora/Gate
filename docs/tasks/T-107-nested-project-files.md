# T-107: a nested project's files are not judged by the root project's rules

| Field | Value |
| --- | --- |
| Status | open |
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

## Resume point

Not started.

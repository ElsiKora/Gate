# T-083: Release-candidate trial

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 |
| Writer | Gate 001 |
| Reviewer | the lead |
| Depends on | the stack T-048, T-059, T-061, T-052 (merged at `2ef2cc6`) |
| Touches | `docs/tasks/T-083-rc-trial.md` only |

## Goal

`main` at `2ef2cc6` is used the way a new repository would use the 0.3.0 release, before the release
is cut: every confusing message, wrong advice, crash or slow step a user meets is written down here,
ranked, so the lead can decide what to fix before 0.3.0. Nothing is fixed in this task.

## Scope

- Build `main`'s `gate` and every plugin once, in one lane job.
- Outside the lane, on synthetic repositories written for the trial in a scratch folder outside this
  repository: one repository that is a single project, and a pnpm monorepo. Between them they hold a
  Node and TypeScript app on the `frontend` pack `nextjs`, a NestJS service, a CDK app, image and
  compose files, a documentation tree and CI workflows.
- Run `gate init`, `gate lock`, `gate sync`, `gate check` (with `--timings`), `gate doctor`, and each
  plugin's `recommended` and `adoption` presets, with the freshly built plugins in place of the
  released ones.
- Not in scope: fixing what is found; the findings are the lead's to schedule.

## Done when

- The findings below are ranked, each with the command that showed it and its output.
- The task file is pushed and the lead has the findings.

## Notes

The synthetic trees are written by a script kept outside this repository; every name in them is
invented for the trial.

## Findings

Ranked: **blocker** (a user cannot get through), **major** (wrong advice or a crash), **minor**
(confusing or slow), **nit**.

(filled in by the trial)

## Resume point

2026-11-20T23:30:00+00:00: branch cut from `main` at `2ef2cc6`; the build of `gate` and every plugin
waits in the lane (ticket 390); the synthetic trees are written. Next: the trial once the build is
done.

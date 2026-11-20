# T-083: Release-candidate trial

| Field | Value |
| --- | --- |
| Status | in review |
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
- Run `gate init`, `gate lock`, `gate sync`, `gate check` (with `--timings`), `gate doctor`, and
  each plugin's `recommended` and `adoption` presets, with the freshly built plugins in place of the
  released ones.
- Not in scope: fixing what is found; the findings are the lead's to schedule.

## Done when

- The findings below are ranked, each with the command that showed it and its output.
- The task file is pushed and the lead has the findings.

## Notes

The synthetic trees are written by a script kept outside this repository; every name in them is
invented for the trial.

## Findings

Ranked: **blocker** (a user cannot get through), **major** (wrong advice, a wrong result or a step
that fails a gate), **minor** (confusing or slow), **nit**. Each names the command of the trial that
showed it; every command ran the built `gate` of `2ef2cc6` with the plugins of that build pinned by
`gate lock --set` under their future URLs (`plugin-<name>-v0.3.0`), in a cache of its own. 68
commands in all, between 2026-11-20T23:44 and 23:51.

### Blocker

1. **A tsconfig that extends a file outside the project stops the whole run.** In the monorepo, a
   project's `tsconfig.json` with `"extends": "../../tsconfig.base.json"`, the layout
   `node/tsconfig` requires (every project extends its install root's base), makes every rule that
   resolves imports fail with an environment error, and `gate check` exits 2 with no finding of any
   project: `plugin nestjs in project api (its paths are relative to apps/api): rule barrel-exports
   failed: cannot resolve "./order.module" from src/modules/order/index.ts: Tsconfig not found
   /tsconfig.base.json`. The same for `cdk` (`cdk/stack-names`, path shown as
   `/gate/tsconfig.base.json` for `"extends": "../tsconfig.base.json"`) and `frontend`
   (`frontend/import-direction`); `typescript/parse` passes. `reads = ["tsconfig.base.json"]` on the
   plugin changes nothing. The message gives no advice. So a pnpm monorepo cannot hold a NestJS, CDK
   or frontend project that meets the `node` standard. Command: `gate check` in the monorepo.

### Major

2. **A cold first run fails the hook budget.** Compiling the plugins counts toward `--fail-after`:
   with nine plugins and nothing compiled, `gate check --staged --warn-after 3 --fail-after 10
   --hook` (the getting-started hook) took 15.4 s and failed with `gate/budget` (exit 1). That is
   the first commit after a clone, a core upgrade or a plugin bump. `gate doctor` says `ready` at
   that point and does not mention that the plugins are not compiled. A full cold `gate check
   --timings` spends 28.9 s (nine plugins) and 29.6 s (ten) in `load`; the rules themselves run in
   under 0.3 s.
3. **`gate explain` and `gate list` ignore presets.** With `extends = ["docker/adoption"]`, `gate
   explain docker/dockerfile` prints `no gate.toml configures it` while the rule runs at `warning`;
   `gate list` prints the `extends` lines but no rule a preset turns on, and no level. The plugins
   guide says `gate list` shows the result.
4. **Advice that names a command that cannot answer it.** An unknown preset (`extends =
   ["frontend/strict"]`) says `plugin frontend has no preset strict; run gate list to see its
   presets`; `gate list` prints no preset of any plugin.
5. **An unknown option value gets the raw schema text.** `pack = "next"` is refused with `plugin
   frontend options: /pack: "next" is not valid under any of the schemas listed in the 'anyOf'
   keyword`. The frontend page says an unknown pack is a configuration error that lists the three
   packs; only a missing pack does (`write pack = "react-vite", "nextjs" or "react-library"`).
6. **`gate lock` blames the credential for a release that does not exist.** With the `0.3.0` sources
   not yet released, `gate lock` stops at the first plugin with `release plugin-ci-v0.3.0 of
   ElsiKora/Gate was not found with the credential from gh auth token (... HTTP 404); check the
   version, and set GATE_GITHUB_TOKEN to a token that can read ElsiKora/Gate`, for a public
   repository and a token that reads it. It names one plugin per run, not every missing one.
7. **The NestJS entry point is a finding by default.** `nestjs/barrel-exports` reports every
   service's `src/main.ts` as `#runtime-entrypoint` (`private-entrypoints` defaults to `[]` while
   `entrypoint-suffix` defaults to `/main.ts`); the help, `list the file in private-entrypoints`,
   does not say that this is an option of the rule entry. Every NestJS project starts with this
   error.
8. **`compose` port advice drops the host port.** For `"5432:5432"` the help suggests
   `"127.0.0.1::5432"`, which publishes the container's port on a random host port; what keeps the
   behaviour is `"127.0.0.1:5432:5432"`.

### Minor

9. **`gate lock --set` takes one plugin per run.** Pinning the nine local builds took nine runs;
   `--set` twice is refused by the argument parser.
10. **A project-scope preset at a root that is not a project takes two errors to place.**
    `[workspace] extends = ["docker/recommended"]` first says to add `[plugins.docker]` at the root;
    after that, `turns on no repository-scope rule ...; extend it in a [[profiles]] entry`.
11. **A warm run still loads every plugin.** With every call answered from the result cache, `load`
    takes 0.9 to 1.9 s of a 1.2 to 2.3 s run (single project, nine plugins).
12. **`--timings` does not split `load` by plugin,** so the slow compile of item 2 cannot be
    attributed.
13. **A malformed header gets three errors with the same help.** `wip` as a message gives
    `type-empty`, `scope-empty` and `subject-empty`, each repeating the full help, and `names no
    type` although `wip` is in the list of types the help prints.
14. **Nine findings for one tsconfig base.** `node/tsconfig-base#flags` reports each missing flag as
    a finding of its own.
15. **`ci` has no adoption preset.** An existing repository with workflows written by hand gets ten
    errors from `ci/recommended` with no warning level to start from; the baseline is the only way
    in.
16. **`docs/layout` findings without help.** `#sections` and `#unknown` print no `help:` line; the
    two `#sections` findings of one page are listed `Entries` before `Reading order`, the reverse of
    the order they ask for.

### Nit

17. `the runtime stage stage 1 runs as root` (and `... does not set ...`): an unnamed stage is named
    `stage 1` after the word `stage`.
18. `gate doctor` before the first `gate sync`: `the hooks in .githooks do not run; run gate sync`,
    when `.githooks` does not exist yet. `gate sync` then prints `set core.hooksPath to .githooks
    (unset)`, where `(unset)` is the earlier value.
19. A check after `gate baseline write` prints `0 errors, 0 warnings` with no count of the findings
    the baseline holds back.
20. `gate/budget` has no `help:` line.
21. The plugins guide says `cdk/adoption` warns only on the renames; the cdk page (correctly) lists
    `properties` and `tests` too.
22. `gate init`'s commented example pins `git@0.2.1`, and `gate --version` says `0.2.1`; the
    documents still mix 0.2.x and 0.3.0 pins. `scripts/release-check.sh` reports all of these at the
    cut, so they are listed only for completeness.

### Worked as documented

`gate init`, `gate lock --set`, `gate sync` (hooks, workflow, eight `docs` seeds), `gate sync
--check`, `gate doctor` after sync, `gate baseline write`, the `--commit-message` run, change
selection with its reasons (`ran api: depends on shared`), the generated workflow under
`ci/recommended` (no finding), the `adoption` presets of `node`, `docker`, `docs`, `repository`,
`frontend` and `cdk`, and the configuration errors for a missing `pack`, a missing `kind` and a
wrongly typed `regions`.

### Not covered

The hooks through `bin/gate`: the launcher fetches the core `gate.lock` pins (released 0.2.1), not
the built one, so the hook commands ran with the built `gate` directly.

## Resume point

2026-11-20T23:51:25+00:00: the trial is done on `main` at `2ef2cc6`: one lane build (ticket 390,
304 s), then 68 commands on the two synthetic repositories; 22 findings, ranked above. Next: the
lead's decision on what to fix before 0.3.0.

# Plan

Gate becomes a modular linter and gate: a core with no rule, and plugins for every rule, generator and
language, pinned by version and checksum. The design is [architecture.md](architecture.md). The
reference for every ported rule is the reference implementation, outside this repository.

## Releases

| Release | Date (target) | Content | Who adopts |
| --- | --- | --- | --- |
| `v0.1.0-rc.1` | 2026-11-18 | core, runtime, SDK, launcher; plugins `typescript`, `nestjs`, `git`, `ci`, `markdown`, `agents`, `ledger`; dry run of the first consumer's move | first consumer, on a private candidate |
| `v0.1.0` | 2026-11-18 | the same, fixed by the dry run; guides; public repository from a clean root | first consumer in CI |
| `v0.2.0` | 2026-11-19 | product plugins ported 1:1 (outside this repository), the other two consumers' moves accepted, review of the release-1 design rows | second and third consumers, at their leads' time |

## Tasks

| Task | Release | Title | Status | Depends on |
| --- | --- | --- | --- | --- |
| T-000 | - | Baseline: the reference implementation's runs and timings (outside the repository) | done | none |
| T-001 | - | Measure the plugin form | done: wasm components; phases 0.94-1.65x native, 23% less memory (spike/RESULTS.md) | none |
| T-002 | rc.1 | Host foundations: workspace layout, CLI, configuration, projects, snapshot, change selection, findings, report | done (reviewed, one fix round) | none |
| T-003 | rc.1 | Plugin interface (WIT), runtime, SDK, fixture harness | done (reviewed, fix round; findings in T-013) | T-001 |
| T-004 | rc.1 | Distribution: lock, verified fetch, cache, launcher, release workflow | done (reviewed, one fix round) | T-002 |
| T-005 | rc.1 | Sync and run: hooks, CI jobs, steps, suites, tools, doctor | done (reviewed, fix round, last list) | T-002 |
| T-006 | rc.1 | TypeScript language library and plugin | done (reviewed, fix round; acceptance A–D identical on main's host) | T-003 |
| T-007 | rc.1 | Plugin `nestjs`: T-007a crate and structure rules, T-007b semantic rules | done | T-006 |
| T-008 | rc.1 | Plugins `git`, `ci`, `markdown`, `agents` | done (no unexpected difference on main's gate; `rule-enforcement` partial until every plugin loads) | T-003 |
| T-009 | rc.1 | Plugin `ledger` | done (C 33 identical, 6 wait for nestjs on main; D 46/46; reviewed, fix round) | T-003 |
| T-010 | rc.1 | Acceptance against the reference implementation (outside the repository); first consumer dry run | done (first consumer green on rc.1: list, sync --check and check exit 0) | T-002 |
| T-011 | v0.1.0 | Public presentation: README, licence, changelog, contributing, guides | merged; second pass after T-002, T-003 | none |
| T-012 | v0.2.0 | Product plugins ported 1:1 (outside the repository) | planned | T-007 |
| T-013 | rc.1 | Sync-drift and sync fixes from the T-003 fix-round review; `gate sync --plugin` | done (reviewed, fix round, verified) | T-003 |
| T-015 | rc.1 | sync-state answers no drift under the fallback (history from before Gate is judged by path) | done (reviewed) | T-013 |
| T-014 | v0.1.0 | Host hardening: the v0.1.0 items of "Open after rc.1" (load-failure kinds, deadline and fuel, `--rev`, prune, `gate/sync#` findings) | done (reviewed, one fix round) | T-013 |
| T-016 | rc.1 | Reproducible components: one package per build, paths remapped, deterministic archives, CI check | done (reviewed, one fix round) | T-004 |
| T-018 | rc.1 | Version `0.1.0-rc.1` for the core and every plugin, so the release workflows accept the tags | done | T-016, T-017 |
| T-017 | rc.1 | `[ci.github] aggregate`; an unknown `[generate]` generator is a configuration error at load | done (reviewed, one fix round) | none |
| T-019 | v0.1.0 | A schema for `[plugins.<name>] options`, checked at load | done (reviewed, one fix round) | T-014 |
| T-020 | v0.1.0 | `git/branch-name` and subject forms of `git/main-history` | done (reviewed, one fix round) | T-019 |
| T-021 | v0.1.0 | Ledger count generators and repository-root citations | done (reviewed, one fix round) | none |
| T-022 | v0.1.0 | Hook arguments for steps, tools per step, release notes | done (reviewed, one fix round) | none |
| T-023 | v0.1.0 | The launcher finds a private asset in pretty-printed release JSON | done | none |
| T-024 | v0.1.0 | Public pass: no old tool, no converter; wording checked against the code at the fresh root | done (second part reviewed) | none |
| T-025 | v0.2.0 | A full check is fast cold and the result cache makes a warm run cheap | in progress | none |
| T-026 | v0.2.0 | `typescript/barrel-imports` | in progress (reviewed, fix round passes the plugin's gates) | none |
| T-027 | v0.1.0 | Version 0.1.0 and its changelog | done | T-024 |
| T-029 | v0.2.0 | `git`: branch names and commit messages as ElsiKora's public linters check them | in progress | none |

## Open after rc.1

From the reviews of T-003 and T-005, kept out of rc.1 on purpose:

- Listing cost: `paths()` scans the snapshot per call and `Tree::contains` lists per probe (v0.2.0).
- The plugin read record as a result-cache key: plugin sha256, rules, options, change set (v0.2.0).
- `gate cache prune --older-than | --max-size` (v0.2.0); the changelog in release notes (v0.1.0).
- Commit answers other than sync-drift persist in the cache (v0.2.0).

From the first consumer's move (2026-11-18):

- `git/main-history` expresses subject-form policies: a subject regex and a parent count per commit form,
  with the merge-body, sync-merge and release checks optional (done, T-020).
- No rule checks the current branch name; `branch-pattern` and `branch-length` judge only the name a
  merge subject captures. A `git/branch-name` rule, with `main` and a detached HEAD passing (done, T-020).
- `agents/instruction-references` resolves repository-root citations that name another project
  (done, T-021).
- `gate run` installs only the `[tools]` a step uses, so a cheap hook neither downloads unrelated
  tools nor fails offline because of them (done, T-022).
- Hook arguments reach run steps (the commit-msg file, the pre-push refs) (done, T-022).
- The generated CI workflow passes an optional `GATE_GITHUB_TOKEN` secret, for private plugin
  releases outside the repository; it uses `github.token`, which reads the repository's own releases
  (v0.1.0: `GATE_GITHUB_TOKEN` from `github.token`).
- `[tools]` URLs take the same local http exception as `gate lock`, for test fixtures (done, T-022).
- The plugin interface declares a schema for `[plugins.<name>] options`, as it does per rule and per
  generator, so the host validates them at load: today a missing option surfaces only when a rule runs,
  as an environment error (done, T-019; an option only a rule needs is still reported when the rule
  runs, T-029).
- The `ledger` plugin has no generators; the second consumer's configuration names
  `ledger/decision-counts` and `ledger/review-progress` (done, T-021).
- The plugin guide names `scripts/build-component.sh` as the way to build a plugin reproducibly (done, T-022).
- The public pass (T-024): architecture §14 says only that Gate reads its own configuration, and the
  public history starts at a fresh root (done, T-024).

From the second consumer's requirements (2026-11-18):

- `typescript`: a name that its folder's barrel does not export is imported only from inside that
  folder; kind children stay importable within their owner; test roots are outside unless configured
  exempt (v0.2.0, T-026).
- A full `gate check` with every plugin, cold cache, is no slower than the reference tool on the same
  tree; `--timings` and `--budget` stay (release criterion of each move; T-025). Measured on its
  converted tree with rc.1, cold, `gate check --rule '<plugin>/*' --timings`: git over 90 s, nestjs
  38.3 s, ledger 12.0 s, typescript 5.2 s, the others under 2 s; all together 507 s, against 6.99 s.
- `nestjs`: a table whose owner declares a judge routine is filtered by the judge's columns only through
  that judge (after v0.2.0).
- `nestjs`: a registered interval carries a typed kind and reaches the mechanism the configuration binds
  to that kind (after v0.2.0).
- Egress follows the transitive package closure; `ledger` closes a record against a counterpart record
  read through a `reads` grant outside the project (after v0.2.0).

## Decisions

- D1: the Rust workspace sits at the repository root.
- D2 (2026-11-16, owner): the core holds no rule; every rule, generator and language is a plugin.
- D3 (2026-11-16, owner): no compatibility or legacy layer.
- D4: the public history starts at a fresh root commit at `v0.1.0`; nothing before it is published.
- D5 (2026-11-18): a task merges only after its gates pass on the exact commit that merges; a branch
  that takes another change after its last green run runs them again. Four test fixtures of T-014 and
  T-019 pinned `0.1.0` and passed only before the rc.1 version moved.

## RESUME POINT

2026-11-18 23:30 UTC: T-001 to T-024 and T-027 merged; main carries version 0.1.0 (merge of
release/v0.1.0). Its gates: the full round (release build, shellcheck,
actionlint), the short round (fmt, clippy, wasm32 clippy, deny, leak scan) and the
targeted round (fmt, clippy, the plugins tests 40 of 40, leak scan), all exit 0. Next: the
fresh public root, the v0.1.0 tags one at a time, the first consumer's lock on v0.1.0. In progress for
v0.2.0: T-025 (speed), T-026 (barrel imports), T-029 (branch and commit format), the product plugins
outside the repository (T-012) and their acceptances. The task table above is the state.

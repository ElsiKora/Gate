# Plan

Gate becomes a modular linter and gate: a core with no rule, and plugins for every rule, generator and
language, pinned by version and checksum. The design is [architecture.md](architecture.md). The
reference for every ported rule is the reference implementation, outside this repository.

## Releases

| Release | Date (target) | Content | Who adopts |
| --- | --- | --- | --- |
| `v0.1.0-rc.1` | 2026-11-18 | core, runtime, SDK, launcher; plugins `typescript`, `nestjs`, `git`, `ci`, `markdown`, `agents`, `ledger`; dry run of the first consumer's move | first consumer, on a private candidate |
| `v0.1.0` | 2026-11-18 | the same, fixed by the dry run; guides; public repository from a clean root | first consumer in CI |
| `v0.2.0` | 2026-11-19, released (tag 17:01Z) | plugin interface `gate:plugin@0.2.0` and every plugin at 0.2.0; `git` branch and commit format at parity with the house linters; the result cache hit by a warm run; stable fingerprints and `gate baseline prune`; option errors as configuration errors; a call state shared by the rules of one call; wasmtime 50.0.2 (CHANGELOG.md, 0.2.0) | second and third consumers, at their leads' time |
| `v0.2.1` | not set | from the `v0.2.0` tag: T-037, T-042, T-043, T-044, T-046, T-047, T-049; documents T-050, T-051, T-054 | the consumers on 0.2.0 |
| `v0.3.0` | not set | breaking changes: T-048 (a configuration table renamed) and T-052 after it, T-053 (`--sha256` removed) | not set |

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
| T-011 | v0.1.0 | Public presentation: README, licence, changelog, contributing, guides | done (merged before the public root) | none |
| T-012 | v0.2.0 | Product plugins ported 1:1 (outside the repository) | outside this repository; no task file here | T-007 |
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
| T-024 | v0.1.0 | Public pass: no old tool, no converter; wording checked against the code at the fresh root | done (second part reviewed; merged before the public root) | none |
| T-025 | v0.2.0 | A full check is fast cold and the result cache makes a warm run cheap | done for v0.2.0 (merged at `264264f` and `b13c5bd`); the cold bound is open and moves to T-047 | none |
| T-026 | v0.2.0 | `typescript/barrel-imports` | done (merged at `60651a1`) | none |
| T-027 | v0.1.0 | Version 0.1.0 and its changelog | done | T-024 |
| T-029 | v0.2.0 | `git`: branch names and commit messages as ElsiKora's public linters check them | done (merged at `e66abc8`) | none |
| T-030 | v0.2.0 | `nestjs` runs within the check's time bound on a large tree | done (merged at `1b7fb64`); the rules' own cost went to T-037 | none |
| T-031 | v0.2.0 | A finding in a nested project's option file names its path once | done (merged at `b72b561`) | none |
| T-032 | v0.2.0 | The newcomer path works as written | done (merged at `8157571`; proof on the released v0.2.0 merged at `105ca02`) | none |
| T-033 | v0.2.0 | The generated checkout keeps no credentials | done (merged at `cc8ee5f`) | none |
| T-034 | v0.2.0 | Stable finding fingerprints and a baseline that only shrinks | done (merged at `1583d85`) | T-029 |
| T-035 | v0.2.0 | Version 0.2.0 and its changelog | done (merged at `89c42c7`, the `v0.2.0` tag) | T-025, T-030, T-031, T-032, T-034, T-036 |
| T-036 | v0.2.0 | header-pattern, ignores and default-ignores of `git/commit-message` | done (merged at `62b59fa`) | none |
| T-037 | v0.2.1 | The `nestjs` rules' own work fits the check's bound on a large tree | done on its branch (`97ca064`), not merged; the rest of the cold bound is T-047 | T-030 |
| T-038 | v0.2.0 | A rule's option error is a configuration error | done (merged at `8aca931`) | T-036 |
| T-039 | v0.2.0 | The release branch compiles again | done (merged at `ef099bc`) | none |
| T-040 | v0.2.0 | wasmtime 50.0.2, for eight security advisories | done (merged at `b3b2fe3`) | none |
| T-041 | v0.2.0 | The error-logging page says what the rule accepts | done (merged at `b6322aa`) | none |
| T-042 | v0.2.1 | prohibited-trailers of `git/commit-message` | in progress (branch at `2dc41a1`) | none |
| T-043 | v0.2.1 | A concurrency group in the generated GitHub workflow | in progress (branch at `d35865f`) | none |
| T-044 | v0.2.1 | `nestjs/wrappers` names own-parameter reads and adapters of interface ports | in progress (branch at `0576b2b`) | none |
| T-045 | v0.2.0 | The documents show the first commit | done (merged at `2ae4d79`, after the tag; documents only) | none |
| T-046 | v0.2.1 | A download that stops sending fails within a bound | open (task file on its branch at `104a644`) | none |
| T-047 | v0.2.1 | The cold compile cost: a cold run compiles every configured plugin, although `--rule` selects one | open (no task file yet; named by T-025 and T-037) | T-025, T-037 |
| T-048 | v0.3.0 | A rule set below its default names its decision | in progress (design approved; branch at `1129031`) | T-043 |
| T-049 | v0.2.1 | A release of `git/main-history` that moves a plugin's pin | design approved; code after T-042 merges (branch at `1739c25`) | T-042 |
| T-050 | v0.2.1 | The plan and the task files say what v0.2.0 merged | in review | none |
| T-051 | v0.2.1 | Hooks written by hand, in the configuration guide | in progress (branch at `922aed5`) | none |
| T-052 | v0.3.0 | not written yet; T-048's release line names it beside T-048 | open (no task file yet; writer Gate 001, after T-048) | T-048 |
| T-053 | v0.3.0 | Pin a local build under its future URL, and check exactly that file | design approved; code after T-044 merges (branch at `5379eed`) | T-044 |
| T-054 | v0.2.1 | The 0.1.0-to-0.2.0 baseline upgrade, written down | done (merged at `4c55ca6`, after the tag; documents only) | none |

## Open after rc.1

From the reviews of T-003 and T-005, kept out of rc.1 on purpose:

- Listing cost: `paths()` scans the snapshot per call and `Tree::contains` lists per probe (open:
  not in v0.2.0; `Tree::contains` in the SDK still lists per probe).
- The plugin read record as a result-cache key: plugin sha256, rules, options, change set (done:
  the key in CHANGELOG 0.1.0; a warm run hits it since T-025, v0.2.0).
- `gate cache prune --older-than | --max-size` (open: not in v0.2.0); the changelog in release notes
  (done, v0.1.0).
- Commit answers other than sync-drift persist in the cache (done, T-025, v0.2.0; the commit message
  and the hook state are asked again each run, T-039).

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
  exempt (done, T-026, v0.2.0).
- A full `gate check` with every plugin, cold cache, is no slower than the reference tool on the same
  tree; `--timings` and `--budget` stay (release criterion of each move; T-025, open: the cold
  bound moves to T-047). Measured on its
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

2026-11-19T17:55:18+00:00: `v0.2.0` is released: the tag `v0.2.0` is `main`'s merge `89c42c7`
(T-035), and the seven plugin tags at 0.2.0 point at the same commit. Every task in the table above
whose status says merged rests on the merge commit it names on `main`; T-001 to T-024 and T-027
merged before the public root (`59fb163`, `v0.1.0`). After the tag, `main` took documents only:
T-045 (`2ae4d79`), T-032's proof on the released v0.2.0 (`105ca02`) and T-054 (`4c55ca6`).

In flight for `v0.2.1`, from the `v0.2.0` tag: T-037 (done on its branch, not merged), T-042, T-043
and T-044 (in progress), T-049 (after T-042), T-046 (open), and T-047, the cold cost of compiling
every configured plugin, which T-025 and T-037 leave open and which has no task file yet; the
documents T-050, T-051 and T-054 (merged) go with it. For `v0.3.0`: T-048, then T-052 (open, no task
file yet), and T-053, all breaking. Still open from the lists above: the listing cost of
`Tree::contains`, `gate cache prune --older-than | --max-size`, and the second consumer's cold bound
(T-047). The task table above is the state.

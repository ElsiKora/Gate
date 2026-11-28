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
| `v0.2.1` | 2026-11-20, released (tag 08:16Z) | the tag `v0.2.1` is `main`'s merge `e7d5a27` (T-070); the core, `git` and `nestjs` at 0.2.1 (tags `plugin-git-v0.2.1`, `plugin-nestjs-v0.2.1`); merged under the tag after `v0.2.0`: T-037, T-042, T-043, T-046, T-047, T-049, T-056, T-058, T-060, T-060b, T-070; documents T-045, T-032's proof on the released v0.2.0, T-050, T-051, T-054 (CHANGELOG.md, 0.2.1) | the consumers on 0.2.0 |
| `v0.3.0` | 2026-11-21, released (tag 09:08Z) | the tag `v0.3.0` is `main`'s merge `d2cd512` (T-088b); plugin interface `gate:plugin@0.3.0`, breaking; the core and all 13 plugins at 0.3.0 (tags `plugin-<name>-v0.3.0`); merged under the tag after `v0.2.1`: T-044, T-048, T-052, T-053, T-055, T-057, T-059, T-061, T-062, T-063, T-064, T-065, T-066, T-066b, T-067, T-068, T-069, T-071, T-071b, T-072, T-073, T-074, T-075, T-076, T-077, T-077b, T-078, T-079, T-079b, T-080, T-080b, T-081, T-082, T-083, T-084, T-085, T-086, T-087, T-088, T-088b, T-089, T-090 (CHANGELOG.md, 0.3.0) | not set |
| `v0.3.1` | 2026-11-22, released (tag 22:14Z) | the tag `v0.3.1` is `main`'s merge `c7b40d1` (T-097); the core at 0.3.1, every plugin at 0.3.0, plugin interface `gate:plugin@0.3.0`; merged under the tag after `v0.3.0`: T-091, T-094 (documents), T-095 (CI job `needs` and `when`, `url` 2.5.9) and T-097 (CHANGELOG.md, 0.3.1) | the first consumer's move |

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
| T-025 | v0.2.0 | A full check is fast cold and the result cache makes a warm run cheap | done for v0.2.0 (merged at `264264f` and `b13c5bd`); the cold bound moved to T-047 | none |
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
| T-037 | v0.2.1 | The `nestjs` rules' own work fits the check's bound on a large tree | done (merged at `73d33e7`); the rest of the cold bound was T-047 | T-030 |
| T-038 | v0.2.0 | A rule's option error is a configuration error | done (merged at `8aca931`) | T-036 |
| T-039 | v0.2.0 | The release branch compiles again | done (merged at `ef099bc`) | none |
| T-040 | v0.2.0 | wasmtime 50.0.2, for eight security advisories | done (merged at `b3b2fe3`) | none |
| T-041 | v0.2.0 | The error-logging page says what the rule accepts | done (merged at `b6322aa`) | none |
| T-042 | v0.2.1 | prohibited-trailers of `git/commit-message` | done (merged at `8b9208f`) | none |
| T-043 | v0.2.1 | A concurrency group in the generated GitHub workflow | done (merged at `9b18b32`) | none |
| T-044 | v0.3.0 | `nestjs/wrappers` names own-parameter reads and adapters of interface ports | done (merged at `4c09a04`, after the `v0.2.1` tag) | none |
| T-045 | v0.2.0 | The documents show the first commit | done (merged at `2ae4d79`, after the tag; documents only) | none |
| T-046 | v0.2.1 | A download that stops sending fails within a bound | done (merged at `389a932`) | none |
| T-047 | v0.2.1 | A run compiles only the plugins it needs | done (merged at `340cf8b`) | none |
| T-048 | v0.3.0 | A rule set below its default names its decision | done (merged at `f7ee1c1`) | T-073 |
| T-049 | v0.2.1 | A release of `git/main-history` that moves a plugin's pin | done (merged at `ec66737`) | T-042 |
| T-050 | v0.2.1 | The plan and the task files say what v0.2.0 merged | done (merged at `37f3420`; documents only) | none |
| T-051 | v0.2.1 | Hooks written by hand, in the configuration guide | done (merged at `f0d7ca4`; documents only) | none |
| T-052 | v0.3.0 | Two check-time thresholds, a warning and a failure | done (merged at `2ef2cc6`) | T-061 |
| T-053 | v0.3.0 | Pin a local build under its future URL, and check exactly that file | done (merged at `6636e3e`) | none |
| T-054 | v0.2.1 | The 0.1.0-to-0.2.0 baseline upgrade, written down | done (merged at `4c55ca6`, after the tag; documents only) | none |
| T-055 | v0.3.0 | `git/commit-message` checks the pushed commits too | done (merged at `7c907b0`) | T-042 |
| T-056 | v0.2.1 | `gate cache prune` bounds the whole cache | done (merged at `e9e1434`) | none |
| T-057 | v0.3.0 | A `--commit-message` check checks only the message | done (merged at `308eca9`) | T-047 |
| T-058 | v0.2.1 | Answer `Tree::contains` and prefix listings without scanning the tree | done (merged at `cf97401`) | none |
| T-059 | v0.3.0 | The decision a rule entry names is an accepted decision of the index | done (merged at `835f83f`) | T-048 |
| T-060 | v0.2.1 | Build each test component once per test binary | done (merged at `08668a8`; tests only) | none |
| T-060b | v0.2.1 | Polish the test component memo | done (merged at `a3dd8d5`; tests only) | T-060 |
| T-061 | v0.3.0 | An enabled doc rule fails on a missing ledger or agent docs | done (merged at `b806641`) | T-059 |
| T-062 | v0.3.0 | One documentation layout for every project, a standard per kind: the `docs` plugin, seeds | done (merged at `7b525d6`) | none |
| T-063 | v0.3.0 | The ledger and agents plugins seed their own minimum | done (merged at `2f4fbf0`) | T-061, T-062 |
| T-064 | v0.3.0 | A `cdk` plugin, one standard for every AWS CDK app | done (merged at `0a3d5a8`) | T-062 |
| T-065 | v0.3.0 | One Node toolchain standard per kind, checked by a `node` plugin | done (merged at `7a5088f`) | T-062 |
| T-066 | v0.3.0 | Containers and repository hygiene: the `docker` and `repository` plugins, `tree.size` | done (merged at `cee6491`) | the `gate:plugin@0.3.0` bump |
| T-066b | v0.3.0 | Secret identity, key bodies and filtered sizes | done (merged at `16d9431`) | T-066 |
| T-067 | v0.3.0 | A standard for CI workflows, checked by the `ci` plugin | done (merged at `de88866`) | none |
| T-068 | v0.3.0 | A `frontend` plugin, one standard for React apps and libraries | done (merged at `f611036`) | none |
| T-069 | v0.3.0 | Every shared plugin ships two presets | done (merged at `6b36eea`) | none |
| T-070 | v0.2.1 | Version 0.2.1 and its changelog | done (merged at `e7d5a27`, the `v0.2.1` tag) | none |
| T-071 | v0.3.0 | A workspace root extends presets for its repository-scope rules | done (merged at `d0c6cda`) | none |
| T-071b | v0.3.0 | The workspace preset test after T-057 | done (merged at `30a1da2`) | T-057, T-071 |
| T-072 | v0.3.0 | Exceptions and rule decisions at a workspace root that is not a project | done (merged at `3726757`) | T-059, T-071 |
| T-073 | v0.3.0 | wasmtime 50.0.3, for RUSTSEC-2026-0587 | done (merged at `7f62554`) | T-074 |
| T-074 | v0.3.0 | Gate checks itself | done (merged at `5d7f1b6`) | none |
| T-075 | v0.3.0 | Prohibited body lines and a body full stop above the trailers | done (merged at `8b9d40c`) | T-074 |
| T-076 | v0.3.0 | Concurrent `gate lock` runs keep both changes | done (merged at `1a32c10`) | none |
| T-077 | v0.3.0 | `gate cache prune` against a concurrent `gate lock --set` | done (merged as `4f3548c9`) | T-085 |
| T-077b | v0.3.0 | `gate/baseline.json` is replaced whole, and its writers take turns | done (merged at `db96d37`) | none |
| T-078 | v0.3.0 | Release documentation for 0.3.0 | done (merged at `18c0bf8`) | none |
| T-079 | v0.3.0 | The plan says what `main` holds for 0.3.0 | done (merged at `4d1f4fe`) | none |
| T-079b | v0.3.0 | Task files say done where `main` holds them | done (merged at `1cbd694`; no task file) | T-079 |
| T-080 | v0.3.0 | An exception is no stale error where its rule did not look | done (merged at `6d7081a`) | none |
| T-080b | v0.3.0 | Refuse an exception's commit id only where it is one | done (merged at `295eaef`) | T-080 |
| T-081 | v0.3.0 | Release cut preparation: one version everywhere | done (merged at `9eeb608`) | none |
| T-082 | v0.3.0 | A 0.3.0 section in every plugin CHANGELOG | done (merged at `249f0b4`) | none |
| T-083 | v0.3.0 | Release-candidate trial | done (merged at `3ce3be6`) | none |
| T-084 | v0.3.0 | A tsconfig base outside the project | done (merged at `2048070`) | none |
| T-085 | v0.3.0 | Release-candidate message fixes | done (merged at `f319843`) | T-083 |
| T-086 | v0.3.0 | Plugin load time, cold and warm | done (merged as `2adfb09d`) | T-083 |
| T-087 | v0.3.0 | Two plugin defaults from the release-candidate trial | done (merged at `e008106`) | T-083 |
| T-088 | v0.3.0 | Three minor findings of the release-candidate trial | done (merged at `d24356f`) | T-083 |
| T-088b | v0.3.0 | The report counts the findings the baseline holds back (trial finding 19) | done (merged at `0474c15`; its test fix at `d2cd512`, the `v0.3.0` tag) | T-080, T-088 |
| T-089 | v0.3.0 | The plan says what `main` holds after T-063 | done (merged at `cf84f39`) | none |
| T-090 | v0.3.0 | Version 0.3.0 for the core and every plugin, and its changelog headings | done (merged at `3e285e1`) | T-089 |
| T-091 | - | Gate checks itself with the released 0.3.0; the plan says what `v0.3.0` holds | done (merged at `9b7bdfd`) | T-090 |
| T-092 | - | A rule option that fails a `oneOf` prints the validator's raw text (`... is not valid under any of the schemas listed in the 'oneOf' keyword`, seen on `"git/branch-name"` with `options = {}`); the same class as trial finding 5, which T-085 fixed for enums | done (reviewed, one fix round, gates green on `93ec2e1`; merged for the next core release) | none |
| T-093 | - | `nestjs/wrappers`: the arguments of a message-factory call that is a Nest exception's message are static (a literal, an enum member or a class identifier, followed through the rule's provenance), so no error code is built from runtime data; a justified case is a rule exception | open (backlog, no release planned) | none |
| T-094 | - | The guide says how a repository that pins a plugin of its own adopts a core release that changes the plugin interface (an upgrade branch with a pre-release build); the hooks text says `--commit-message` checks only the message | done (documents only, merged on `main` after `v0.3.0`) | none |
| T-095 | v0.3.1 | A `[ci.github.jobs.<id>]` job runs after other generated jobs: `needs` and `when = "succeeded" \| "completed"`, written as GitHub `needs:` and `if:`; additive, the plugin interface stays `gate:plugin@0.3.0` | done (reviewed, one fix round; gates green on `21bdc78`; includes `url` 2.5.9 for RUSTSEC-2026-0592) | none |
| T-096 | - | `git/main-history`: remove `release-all-pins` in a future release; the upgrade-branch path of T-094 covers an interface change, and the option admits the wide form in every release commit | open (backlog, no release planned) | T-094 |
| T-097 | v0.3.1 | Version 0.3.1 for the core and its changelog; every plugin stays at 0.3.0 and the plugin interface at `gate:plugin@0.3.0` | done (gates green on `38bd6fe`; the `v0.3.1` tag) | T-095 |
| T-098 | next plugin release | A cold check without history under the first consumer's hook bound with a margin (at most 4000 ms counted, against 6973 ms on 0.3.0): each file read once per call, faster regex engines, ledger and TypeScript call state shared by a call's rules, nestjs hot spots; findings unchanged | done (reviewed, one fix round, gates green on `a15a3180`; 4075 ms counted at the median of eight cold runs; merged for the next release) | T-097 |
| T-099 | next | The generated CI can keep the Gate cache between runs (`[ci.github] cache = true`, opt-in; the core and tools never cached, plugins verified by sha256), cancels superseded runs of every ref but the default branch, and takes `[ci.github] aggregate-name`; a plugin kept in the repository it checks is published before its pin | done (reviewed, one fix round, gates green on `08d1c7f`; merged for the next release) | none |
| T-100 | next | `gate check --skip-rule <id>` (repeatable, `<plugin>/*` on the command line), also in hook and CI-job `check` entries by single rule id: leaves rules out of a run after `--rule`; a key that matches no enabled rule is an error; the summary names the skipped rules. There is still no history switch | done (reviewed, one fix round, gates green on `f65ecee`; merged for the next release) | none |
| T-101 | v0.3.2 | Version 0.3.2 for the core; `cdk`, `frontend`, `ledger`, `nestjs` and `typescript` at 0.3.1 (their wasm bytes changed in T-098), the other eight plugins at 0.3.0, the plugin interface at `gate:plugin@0.3.0`; the changelogs | done (gates green on `b13b28f`, 1631 tests passed; merged; tagged v0.3.2 and the five plugins at 0.3.1) | T-098, T-099, T-100 |
| T-102 | nestjs 0.3.2 | `nestjs/raw-config-access`: the first parameter of an element callback (`map`, `flatMap`, `forEach`, `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `some`, `every`) over a literal array of strings names its keys, as a `for ... of` variable does; a getter whose first argument can never be a key (an object, array, function or class expression, a non-string literal) is no read ; a written name or an empty array names no key | done (reviewed, one fix round; gates green on `ae6df72`, 1636 tests passed; only nestjs bytes change; five findings fewer and none new on the first consumer; release commit `e251b76` green; merged; tagged plugin-nestjs-v0.3.2) | none |
| T-103 | - | Gate checks itself with the released 0.3.2: `[gate] version = "0.3.2"`, the core's pins in `gate.lock`, the generated workflow cancels superseded runs off the default branch; `git`, `agents`, `markdown` and `ci` stay at 0.3.0 (no later release); no `cache` or `aggregate-name` | done (reviewed by the lead; checks green under 0.3.2; merged) | T-101 |
| T-104 | next | Under `[decision-policy] require-decision`, a rule or aspect that no preset names has the default `error`, so lowering it to `warning`, `note` or `off` needs a decision | done (reviewed, one fix round; gates green on `4b40022`; merged for the next core release) | none |
| T-105 | next ledger | A decision named in a nested project's `gate.toml` is checked against the workspace's decision index, once, by one rule; the plugin interface stays `gate:plugin@0.3.0` | done (reviewed, one fix round and one correction; the repository call carries every written decision and `ledger/config-decisions` replaces four rules; a repository-scope rule a project turns on and the root never names is a configuration error; ledger 0.3.2 and core 0.3.3 need each other; gates green on `0f8e24db`, 1667 tests passed; merged for the next release) | none |
| T-106 | next | `gate list` prints the repository-scope rules a preset turns on, from the same rule set `gate check` runs | done (reviewed, one fix round; gates green on `c04fed22`, 1660 tests passed; merged for the next core release) | none |
| T-107 | next | A root project's rule never reports a file that a nested project owns (seen with `typescript/barrel-imports`) | done (reviewed, one fix round; the cause was in the core: findings of a project call on a nested project's files are dropped; gates green on `263cb2d`, 1665 tests passed; merged for the next core release) | none |
| T-108 | v0.3.3 | Version 0.3.3 for the core and 0.3.2 for `ledger`, with their changelogs | done (gates green on `8e46d138`, 1667 tests passed; only `ledger` changes bytes from code; merged, tagged `v0.3.3` and `plugin-ledger-v0.3.2`) | T-104, T-105, T-106, T-107 |
| T-109 | next ledger | The owner every exception names, a nested project's included, is judged once against the root's implementation records: one repository-scope rule, `ledger/exception-owners`, replaces the project rule and its root counterpart, and the repository call lists every exception | done (reviewed, one fix round; gates green on `71c20d13`, 1674 tests passed; merged for core 0.3.4 and ledger 0.3.3, which need each other) | T-105 |
| T-110 | - | Gate checks itself with the released 0.3.3: `[gate] version = "0.3.3"`, the core's pins in `gate.lock`; the four plugin pins stay at 0.3.0 | done (lock, sync and check exit 0; sync writes nothing) | T-108 |
| T-111 | v0.3.4 | Version 0.3.4 for the core and 0.3.3 for `ledger`, with their changelogs | done (gates green on `67f3a135`, 1674 tests passed; only `ledger` changes bytes from code; merged, tagged `v0.3.4` and `plugin-ledger-v0.3.3`) | T-109 |
| T-112 | - | Gate checks itself with the released 0.3.4: `[gate] version = "0.3.4"`, the core's pins in `gate.lock`; the four plugin pins stay at 0.3.0 | done (lock, sync and check exit 0; sync writes nothing) | T-111 |
| T-113 | next | The generated CI cache survives a move of `gate.lock`: a job restores its newest entry across a pin move, plugins and results still verified by their own keys | open (backlog) | none |
| T-114 | next nestjs | `nestjs/kind-imports`: how code imports the files of kind folders and the rest of a resource, a new rule in no preset | open | none |
| T-115 | next nestjs | `nestjs/typeorm-lock-owner`: every TypeORM row lock comes from one owner function through recognised read shapes, a new rule in no preset | done (one fix round; fmt, clippy, test 1694 passed, deny and release build exit 0 on `52901d53`; merged) | none |

## Open after rc.1

From the reviews of T-003 and T-005, kept out of rc.1 on purpose:

- Listing cost: `paths()` scans the snapshot per call and `Tree::contains` lists per probe (done,
  T-058, v0.2.1: a listing of a prefix is a binary search).
- The plugin read record as a result-cache key: plugin sha256, rules, options, change set (done:
  the key in CHANGELOG 0.1.0; a warm run hits it since T-025, v0.2.0).
- `gate cache prune --older-than | --max-size` (done, T-056, v0.2.1); the changelog in release notes
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
  tree; `--timings` and the time thresholds stay (release criterion of each move; T-025, then T-047,
  merged at `340cf8b`: a check compiles only the plugins it runs; this plan records no measurement
  of the full cold check after it). Measured on its
  converted tree with rc.1, cold, `gate check --rule '<plugin>/*' --timings`: git over 90 s, nestjs
  38.3 s, ledger 12.0 s, typescript 5.2 s, the others under 2 s; all together 507 s, against 6.99 s.
- `nestjs`: a table whose owner declares a judge routine is filtered by the judge's columns only through
  that judge (backlog, after v0.3.0).
- `nestjs`: a registered interval carries a typed kind and reaches the mechanism the configuration binds
  to that kind (backlog, after v0.3.0).
- Egress follows the transitive package closure; `ledger` closes a record against a counterpart record
  read through a `reads` grant outside the project (backlog, after v0.3.0).

## Decisions

- D1: the Rust workspace sits at the repository root.
- D2 (2026-11-16, owner): the core holds no rule; every rule, generator and language is a plugin.
- D3 (2026-11-16, owner): no compatibility or legacy layer.
- D4: the public history starts at a fresh root commit at `v0.1.0`; nothing before it is published.
- D5 (2026-11-18): a task merges only after its gates pass on the exact commit that merges; a branch
  that takes another change after its last green run runs them again. Four test fixtures of T-014 and
  T-019 pinned `0.1.0` and passed only before the rc.1 version moved.

## RESUME POINT

2026-11-21T09:25:30+00:00: `v0.3.0` is released: the tag `v0.3.0` is `main`'s merge `d2cd512` (T-088b's
test fix), tagged 09:08Z, with the 13 `plugin-<name>-v0.3.0` tags. `v0.2.1` is the tag on `main`'s
merge `e7d5a27` (T-070), with `plugin-git-v0.2.1` and `plugin-nestjs-v0.2.1`. Every task the table
marks done names its merge commit on `main`, from `git log --merges`; T-001 to T-024 and T-027
merged before the public root (`59fb163`, `v0.1.0`).

Under the `v0.3.0` tag, merged after `v0.2.1`: T-044, T-048, T-052, T-053, T-055, T-057, T-059, T-061, T-062, T-063, T-064, T-065, T-066, T-066b, T-067, T-068, T-069, T-071, T-071b, T-072, T-073, T-074, T-075, T-076, T-077, T-077b, T-078, T-079, T-079b, T-080, T-080b, T-081, T-082, T-083, T-084, T-085, T-086, T-087, T-088, T-088b, T-089 and T-090. After
it: T-091 done (merged at `9b7bdfd`); T-094 done (documents only); T-092 open; T-093 and T-096 backlog. The `nestjs` and `ledger` lines of the second
consumer's list stay backlog after `v0.3.0`. The task table above is the state.

# T-005: Sync and run

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | T-002 |
| Touches | `crates/gate-host/src/{sync,run,doctor,tools}/**` (new modules), `crates/gate/src/**` (the `sync`, `run`, `doctor` commands, `check --rule`), `findings.rs`, `workspace.rs` and `config/` for the rule filter and core rule norms, their tests |

## Goal

The core writes and verifies the files that wire Gate into a repository (git hooks, the GitHub Actions
workflow), runs a repository's steps and suites, and checks the working copy's setup with `doctor`
([architecture.md](../architecture.md) sections 10, 11 and 16).

## Scope

1. **Hooks**: from `[hooks]` write `.githooks/<hook>` (POSIX `sh`, executable bit recorded in git) that
   call `bin/gate` with each entry (a gate command: `check --staged`, `check --commit-message "$1"`,
   `check --push` reading the pushed refs from stdin, `run <step>`); `[hooks] budget` adds `--budget`.
   No skip on any environment variable. `gate sync` sets `core.hooksPath=.githooks` for the clone when it
   differs. In a monorepo the hook runs only the projects the change touches and prints which ran and
   which were skipped.
2. **CI**: from `[ci.github]` write the workflow: a first `changes` job that runs
   `gate check --list-changed-projects --format github-output` (add that mode), then the configured jobs
   with `name`, `runs-on`, `services`, `permissions`, setup steps, actions pinned by commit, the gate
   steps (`--budget-fails`, `--no-overrides`, the pushed range right after `--push`), and `if:` skipping
   a job whose declared projects did not change; the job runs when the `changes` job did not succeed.
   The `changes` job runs `gate sync --check`. Stable job ids and names from configuration. `branches`
   are globs passed through as written (default `["main"]`); a job and each `run` step may set
   `working-directory` (the gate commands run at the root); `steps` run before the gate commands and
   `after` steps after them, and only an `after` step takes `if`; a job checks out the whole history
   when a gate command runs a rule that reads history (the plugin runtime says which), checks a pushed
   range, a revision or a change since a base, and any job may set `fetch-depth`.
3. **Sync check**: `gate sync --check` compares every file the core and the plugins' generators would
   write (generators arrive with T-003; leave the seam and a test double) and reports drift as
   `gate/sync#<item>` findings naming the file; a hand edit of a generated file is reported.
4. **Steps and suites**: `gate run <suite|step>`; working directory (project root, or the repository top
   for root steps), environment, `timeout`, `heavy` steps prefixed by `[run] heavy-wrapper` or
   `$GATE_HEAVY_WRAPPER`, `files = "staged" | "changed"` with `glob` appending the matching paths; a
   suite runs every step, prints each exit code and wall time and a summary, fails if any failed;
   `fail-fast` stops at the first.
5. **Tools**: `[tools.<name>]` with version (a label) and per-platform target; download and sha256
   verification reuse T-004's fetch; the unpacked executable's sha256 is recorded and checked before
   each run; `gate cache prune` keeps what each worktree's `[tools]` pins; steps call tools by name.
6. **Doctor**: hooks path set, launcher present, executable and the one this core writes, `gate.lock`
   tracked and matching the configuration (T-004's check), cache directory writable, plugins in the
   lock cached and matching, git version; each check prints ok or what to do.

7. **Rule filter**: `gate check --rule <id>`, repeatable, `<plugin>/<rule>` or `<plugin>/*` (no
   `#<aspect>`), narrows the run to those rules. An id that names no configured rule (no scope declares
   the plugin; for a single rule, no `[rules]` entry names it and the plugin does not declare it) or a
   core rule is a configuration error. The core drops findings of other rules and does not count them
   as run, so their exceptions and baseline entries are not due.
8. **Core rule norms**: the core's own rules (`gate/baseline`, `gate/budget`, `gate/exceptions`,
   `gate/sync`) accept `norm` and `norms` in the root `[rules]`, attached to their findings. `level`,
   `levels`, `off`, `options`, a family `gate/*`, an unknown `gate/<rule>`, or an entry outside the
   root `[rules]` stay errors that say why.

## Done when

- fmt, clippy, test, deny exit 0.
- Tests on synthetic repositories: hooks written and byte-stable; a hand-edited hook reported by
  `sync --check`; the workflow for a two-project fixture with a docs-only change skipping the heavy job
  (assert the `if:` expression and the `changes` output); a suite with one failing step runs all steps
  and exits 1 with every exit code printed; `fail-fast`; a step timeout; heavy wrapper from config and
  from the environment; staged-file steps receive exactly the staged matching paths.
- Tests: a filtered run reports only its rule and no unused exception of the others; a `--rule`
  that names no configured rule, a core rule or an aspect is refused; a core rule's norm and aspect
  norm reach its findings, and its level, options or place outside the root `[rules]` are refused.
- Leak scan prints nothing.

## Notes

- Gate commands in `[hooks]` and `[ci.github] jobs.*.run` are limited to `check ...`,
  `run <name> [--staged|--push]` and `sync --check`, with no shell syntax; `--push` only in `pre-push`,
  `--commit-message` only in the hooks that receive a message file.
- `[ci.github] checkout` and every `uses` must be pinned by a 40-digit commit.
- The `changes` job runs `check --list-changed-projects --format github-output --push "$GATE_RANGE"`,
  the range being the pull request's base or the push's previous tip, zeros (every project) when
  there is neither. A job with `projects` gets `needs: [changes]` and
  `if: ${{ contains(fromJSON(needs.changes.outputs.projects), '<name>') || ... }}`. The workflow sets
  `GATE_GITHUB_TOKEN`.
- Drift findings are `gate/sync#hooks`, `gate/sync#ci` or `gate/sync#<plugin>/<generator>`. A
  generated file that configuration no longer names is found by its marker line, reported by
  `sync --check` and removed by `sync`.
- `config/file.rs` and `config/mod.rs` carry the `[hooks]`, `[run]`, `[ci]` and `[tools]` types and
  checks.
- Tools: `tools::CacheOnly` stands in for T-004's verified download; a mismatched download is not
  stored, a changed cache entry fails closed and is not refetched.
- A two-project fixture without `[workspace]` has the root as project `.`, which owns `docs/`: a
  docs-only change lists `["."]`, so a job declared for `api` and `site` is skipped.

## Resume point

2026-11-16T23:42:18+00:00: last review round done, the target ref always a full ref: `--target-ref` on `check` and `run` (only with `--push`,
carried as `ChangeRequest::Push { target_ref }` into `PushRange.target_ref`; the pre-push hook passes
the remote ref, CI `refs/heads/<base_ref>` or `github.ref`; a value outside `refs/` is refused), `--commit-message` refused in CI jobs, `--flag=value`
refused, no signal lost between spawn and registration, tools built whole with their record and swapped
in, a read-only doctor, process groups signalled through rustix (1.1.6, past RUSTSEC-2026-0486), and the
sync writer's refusals (`.git`, the core's files, the hooks directory, symbolic links). Waiting for the
lead's merge. Once T-003 merges: an end-to-end test of `check --rule`, and the runtime's answer to
`reads_history`.

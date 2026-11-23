# T-100: `--skip-rule` leaves rules out of a run

| Field | Value |
| --- | --- |
| Status | design sent to the lead; no code before the lead's word |
| Release | the next core release (additive; the plugin interface stays `gate:plugin@0.3.0`) |
| Writer | "Gate T-100" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `df718c0` |
| Touches | `crates/gate-host/src/findings.rs` (`RuleFilter`), `crates/gate-host/src/cli.rs` (`--skip-rule`), `crates/gate-host/src/workspace.rs` and `crates/gate-host/src/plugins.rs` (the check of the keys), `crates/gate-host/src/report.rs` (the summary), `crates/gate-host/src/sync/mod.rs` (whether a `check` entry reads history), `crates/gate/src/main.rs`, their tests, `docs/architecture.md`, `docs/guide/configuration.md`, `CHANGELOG.md`, `docs/plan.md`, this task file |

## Goal

A pre-commit hook under a tight bound runs every rule except the history rules (`git/main-history`,
`git/no-ci-skip`), which run in every mode and cost about 10 s cold on a large tree. Today the only
way is a `--rule` list of every other rule, kept in step with the configuration by hand. A general
exclusion selector replaces that list; there is still no history switch.

## Design

`gate check --skip-rule <id>`, repeatable, `<id>` is `<plugin>/<rule>` or `<plugin>/*`, parsed as
`--rule` is (no `#<aspect>`; a core rule `gate/*` is refused: the core's own rules always run).

1. **Selection.** A plugin rule runs when `--rule` allows it (no `--rule`: every rule) and no
   `--skip-rule` key matches it. Skip applies after rule: `--rule git/* --skip-rule
   git/main-history` runs every `git` rule but `git/main-history`. The order of the flags on the
   command line does not matter. One predicate, `RuleFilter::allows`, decides it everywhere `--rule`
   is decided today: which rules a call gets, which findings are kept, which rules count as run,
   which plugin needs (`NEEDS`) are checked, and the run's filter a `sync-state` fallback checks
   the configuration with again.
2. **Exceptions and baseline entries** of a skipped rule are not due in that run, as for a rule
   `--rule` leaves out: the rule is not recorded as run, so `gate/exceptions` and `gate/baseline`
   do not report its entries as unused or stale.
3. **Plugins.** A plugin is not needed (fetched, compiled, called) when `--skip-rule <plugin>/*`
   names it, or when skipping leaves it no rule in a scope (no call is planned for it, so it does
   not load). Its declaration is still checked: validation is never narrowed.
4. **A key that matches nothing is an error** (exit `2`), raised before anything runs, naming the
   key. A `--skip-rule` key must match at least one rule that is enabled in some scope of the
   configuration (any scope, whatever the change selection runs, so a hook does not pass or fail
   by what is staged) and that `--rule` leaves to run:
   - `--skip-rule layers/imports names no configured rule: ...` (no plugin `layers`, or a rule its
     plugin does not declare and no `[rules]` entry names; the same wording as `--rule`);
   - `--skip-rule git/no-ci-skip matches no enabled rule: it is off in every scope; remove the key`
     (configured but off everywhere);
   - `--skip-rule git/no-ci-skip matches no rule --rule leaves to run; remove the key` (with
     `--rule hello/*`).
   A `--rule` key all of whose rules the skip removes is an error too: `--rule git/main-history:
   --skip-rule leaves none of its rules to run`. A hook entry whose skip drifted from the
   configuration fails loudly instead of checking less than it says.
5. **The summary says what was skipped.** Text: `gate check (staged): 12 rules (skipped
   git/main-history, git/no-ci-skip), 0 errors, 0 warnings; projects: ran ...; skipped ...`: the
   enabled rules the skip removed, by id, sorted, each once, so `--skip-rule git/*` lists what it
   actually left out. JSON gets `"skipped-rules": [...]` with the same ids. No skip, no change to
   the line.
6. **Hook and CI-job entries.** A `check` entry of `[hooks]` or of a CI job is parsed with the same
   `CheckArgs`, so `--skip-rule` is accepted there exactly where `--rule` is, and an entry that
   writes a bad value fails when the configuration loads. Whether a generated CI job needs the
   history (`check_reads_history`) reads `--skip-rule` beside `--rule`, so a job that skips every
   history rule does not fetch the history for them.
7. **Result cache.** A call's key is over its request, which lists the rules the call runs; a
   different selection is a different key. A test runs a plugin with two rules in one call fully,
   then with one skipped, then fully again: the skipped rule's findings do not come back from the
   first run's entry, and the third run is answered from the cache with both.

## Tests

- Selection: `--skip-rule` alone, `--skip-rule <plugin>/*`, and with `--rule` (rule then skip),
  on `RuleFilter` and through `gate check`.
- A skip that matches nothing: no plugin, an undeclared rule, a rule off everywhere, a rule `--rule`
  leaves out, a `--rule` key the skip empties, a core rule, an aspect; each exit `2` with its
  message.
- Exceptions of a skipped rule are not reported by `gate/exceptions`, and baseline entries not by
  `gate/baseline`.
- The result cache: the three runs of point 7.
- The summary line and JSON field with and without a skip.
- A hook entry with `--skip-rule` loads; one with a bad value fails at load.

## Done when

- The lead approved the design.
- One lane round on the pushed tip: fmt, clippy `-D warnings`, test, deny exit 0, recorded below.
- `leak-scan.sh` prints nothing; the branch is pushed; the lead has the branch and head commit.

## Proof

(none yet)

## Resume point

2026-11-23T05:37:34+00:00: branch `task/T-100-skip-rule` from `main` at `df718c0`. Design written
above and sent to the lead; waiting for the lead's word before any code.

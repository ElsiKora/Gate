# T-088: Three minor findings of the release-candidate trial

| Field | Value |
| --- | --- |
| Status | round queued |
| Release | v0.3.0 (core and the `node` plugin) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | `main` at `e0081062` |
| Touches | `crates/gate-host/src/doctor/mod.rs`, `crates/gate/src/main.rs`, `crates/gate/tests/cli.rs`, `crates/gate-host/src/config/mod.rs`, `crates/gate-host/src/workspace.rs`, `crates/gate-host/tests/host/config.rs`, `docs/guide/configuration.md`, `plugins/node/src/tsconfig_base.rs`, `plugins/node/tests/node/tsconfig_base.rs`, `plugins/node/rules/tsconfig-base.md`, `CHANGELOG.md` |

## Goal

Minor findings 10, 14 and 18 of the release-candidate trial (T-083):

10. `[workspace] extends = ["docker/recommended"]` at a root that is not a project took two errors
    to place: first "add `[plugins.docker]`", then "turns on no repository-scope rule; extend it in
    a `[[profiles]]` entry".
14. `node/tsconfig-base#flags` reported each flag a base leaves out as a finding of its own: nine
    findings for one file.
18. `gate doctor` before the first `gate sync` said the hooks in `.githooks` do not run when
    `.githooks` did not exist yet, and `gate sync` printed `set core.hooksPath to .githooks
    (unset)`, where `(unset)` was the earlier value.

Not taken:

- 19 (the count of findings the baseline holds back): it is counted in `Findings::finish`, which
  T-080 rewrites, and T-080 changes which findings reach the baseline.
- 21 (the guide's `cdk/adoption` line): already fixed on `main` by T-069; the guide and the cdk
  page both say every rule at `warning`.

## Change

- 18: `gate doctor` says `gate sync` has not written the hooks yet when `.githooks` is not a
  directory, and asks for `gate sync` to write them and set `core.hooksPath`; with `.githooks`
  there it keeps the message that the hooks do not run. `gate sync` prints
  `(it was not set)` or `(it was <path>)`.
- 10: a `[workspace] extends` entry whose plugin the root does not declare names both places in one
  message: the root's `[plugins]` for its repository-scope rules, or a `[[profiles]]` entry with the
  plugin in its `[profiles.plugins]` for its project-scope rules. The configuration is read before
  any plugin loads, so the preset's scope is not known at that point.
- 14: the flags a base leaves out are one finding at the file (key `compilerOptions`) that lists
  every line to add. A flag written with another value stays a finding at its line, keyed by the
  flag, so a base that turns one flag off on purpose holds an exception for that flag alone.

## Tests

- 18: doctor with `[hooks]` and no `.githooks`, and with `.githooks` written but `core.hooksPath`
  unset; the sync line.
- 10: the message names the entry, the root's `[plugins]`, a `[[profiles]]` entry and
  `[profiles.plugins]`.
- 14: one absent flag; several absent flags and both resolution settings in one finding; a flag set
  to `false` and a wrong `module` at their lines next to the grouped finding.

## Resume point

Findings 10, 14 and 18 committed; round queued.

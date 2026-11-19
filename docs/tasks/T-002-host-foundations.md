# T-002: Host foundations

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | none |
| Touches | `Cargo.toml`, `Cargo.lock`, `deny.toml`, `crates/**`, the previous layout (removed), `.gitignore` |

## Goal

The new workspace exists and the core does everything that does not need a plugin: it loads and
validates the root and project `gate.toml` files, discovers projects, opens a snapshot, selects changes,
and turns findings into a report. `gate check` on a repository with no plugins prints a valid empty
report; `gate list` prints projects and configured rules.

## Scope

1. Layout as in [architecture.md](../architecture.md) section 13: remove the previous layout from
   `main`; create `crates/gate` (CLI) and `crates/gate-host`. Keep the workspace lints, the
   pinned toolchain and `deny.toml` (update its licence list as needed).
2. Configuration (section 7): root and project files, `[gate]`, `[workspace]`, `[project]`,
   `[plugins]`, `extends`, `[rules]` (level, norm, options, options-file, per-aspect norms, family
   wildcards like `"ledger/*" = "off"`), `[[exceptions]]`, `[generate]`, `[steps]`, `[suites]`,
   `[hooks]`, `[ci.github]`, `[tools]`. Strict: unknown keys and malformed values are errors naming
   file, key and what is expected. Rule option validation against a JSON Schema is an interface
   (`fn validate(schema, value)`); the schemas come from plugins in T-003. Also a JSON Schema of
   `gate.toml` itself, generated from the types (`gate schema`).
3. Projects (section 8): from `pnpm-workspace.yaml`, `package.json` workspaces, explicit globs, and
   tracked `gate.toml` files; longest-root ownership; the workspace dependency graph for `pnpm`/`npm`
   (dependencies on workspace packages). Cargo and Go discovery may wait; leave the seam.
4. Snapshot: tracked files only, from the working tree, the index (`--staged`) or a commit (`--rev`);
   content ids are git blob ids (from the index or `git ls-tree`; working-tree changes hashed the same
   way). Use the `git` binary through one small module; no libgit2.
5. Change selection: `--changed --base <rev>`, `--staged`, `--push <remote-range>`; the projects
   selected, with the reason, and the projects skipped.
6. Findings and report: the model of section 6; exceptions (match, unmatched exception reported as
   `gate/exceptions`); deterministic order; text output with a one-line summary
   (`gate check (<mode>): N rules, E errors, W warnings; projects: ran a, b; skipped c`), `--format json`,
   `--format sarif`, `--format github`; exit codes 0 (clean), 1 (errors), 2 (configuration or
   environment failure).
7. CLI (clap): `check`, `list`, `explain <rule>`, `schema`; `sync`, `run`, `doctor`, `init`, `lock` exist
   as commands that say "not yet" until T-004/T-005 fill them. `--timings`, `--budget`, `--fail-fast`
   parsed and passed through.

8. Added by the lead during the task (folded into scopes 2, 6 and 7):
   - `[[profiles]]` (`projects` globs with `plugins`, `extends`, `rules`, `generate`, `steps`, `suites`);
     merge order root `[plugins]` → matching profiles in file order → the project's own `gate.toml`;
     `gate list --origin` prints file, line and section of every setting.
   - Root `[steps]` and `[suites]`; step `timeout`, `heavy`, `files = "staged" | "changed"` and `glob`;
     suite `fail-fast`; `[run] heavy-wrapper`.
   - `[exception-policy] require-decision`; exceptions in `gate.exceptions.toml` beside a `gate.toml`,
     as `[[exception]]` entries (`rule`, `target`, `reason`, optional `decision`); `gate schema --exceptions`.
   - Per-aspect `levels` and `norms` tables in a rule entry.
   - A single-project root: no `[workspace]`, or `[workspace] projects` lists `"."`; then the root may
     hold `[project]`, project plugins and project-scope rules. Otherwise a project-scope rule in the
     root's `[rules]` is an error that points to `[[profiles]]`.
   - The baseline `gate/baseline.json` and `gate baseline write`.
   - `--budget` warns, `--budget-fails` errors; `--plugin <name>=<path>` parsed and checked,
     `--no-overrides` refuses it.
   - `[plugins.<name>]` is always a table: `source`, `options` or `options-file` (shared by the
     plugin's rules), `reads`. `[languages.<id>]` per project (opaque JSON objects for now).
     `[[exception]]` takes an optional `owner`; exceptions match by target.

Not in scope: the plugin runtime (T-003), fetching (T-004), hooks/CI/steps (T-005).

## Done when

- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`,
  `cargo deny --locked check`: each exit 0 (heavy runs through the queue).
- Tests (synthetic fixture repositories made with `git init` in a temp dir): configuration errors (one
  per class), project discovery for pnpm, npm workspaces, globs and nested `gate.toml`; ownership by
  longest root; change selection for staged, base range and a dependent project; exception matched and
  unmatched; report order and exit codes.
- `gate check` and `gate list` on a fixture monorepo print the expected text (snapshot tests).
- The leak scan of the local rules prints nothing.

## Notes

- The reference implementation's snapshot, git and history modules (outside this repository) are
  worth reading; take what is good, write it clean.
- Coordinate the crate API with T-003 through the lead: T-003 will need `Snapshot`, the project list,
  the change set and a `Findings` sink from `gate-host`.
- Decisions taken on the way (2026-11-16):
  - Branch: the work is pushed as `task/T-002-host-foundations` (the task-file convention), from the
    local worktree branch.
  - `check` fails closed with exit 2 when a scope that runs declares a plugin: this build has no
    runtime, and a gate that passes without running its rules would let a broken change through.
    T-003 replaces that block in `gate_host::check` with the runtime.
  - Exceptions match by target: the exception's `target` equals the finding's `target` (the rule's own
    grammar, paths relative to the project root), or, for a finding without a target, the finding's
    path once the exception's target is read relative to the directory of the `gate.exceptions.toml`
    (the project root). An exception's line is the line of its `[[exception]]` header. A
    `gate.exceptions.toml` without a `gate.toml` beside it is an error. `owner` is stored and shown;
    that it names an open record is the ledger plugin's check.
  - A bare string under `[plugins]` is an error that shows the table form. `[languages]` may be set in
    a project's `gate.toml` (the root when it is a project) and in `[[profiles]]`; a later layer
    replaces a language's whole table.
  - `require-decision`: a project's own `[exception-policy]` wins, else the root's. The core checks that
    a decision is present; that the decision record exists is a ledger rule's job.
  - Per-aspect settings are `levels`/`norms` tables of the rule entry; a `"<rule>#<aspect>"` key is an
    error that shows the table form (one way to write it). A family wildcard takes only a level;
    precedence aspect > rule > wildcard. A rule without a level keeps its preset's level, else `error`
    (applied by T-003, which owns presets).
  - Merging: the root's `[plugins]` are the repository defaults every project inherits; then each
    matching profile, then the project's file. A rule merges field by field (level, norm, options, each
    aspect), the later layer wins; `extends` are appended without repeats; steps, suites, generators
    and plugins replace by key. A profile that matches no project is an error.
  - Rules, presets, generators and exceptions may name only a plugin declared for their scope (root,
    profile or project). `gate/...` rules are the core's and cannot be configured.
  - Rule scope (repository or project) comes from the plugin's declaration: `Workspace::validate` takes
    it from T-003. A repository-scope rule configured below the root is an error.
  - Without `[workspace]` the root is a project; nested `gate.toml` files are still projects. The root
    project is named `.` unless `[project] name` says otherwise.
  - Budget: an overrun is a `gate/budget` finding, a warning, or an error with `--budget-fails`.
    `gate/...` findings are never written to the baseline. A baseline entry accepts one finding; an
    entry that matches nothing is a `gate/baseline` error, so the baseline only shrinks.
  - Step `cwd` is relative to the project root. CI job keys beyond `[ci.github]` wait for T-005.
  - A change to any file the repository's configuration was read from (the root `gate.toml`, the
    options files it names, workspace manifests outside every project, the root exceptions, the
    baseline) selects every project; a change to what a project's own `gate.toml` read, or to a
    path its plugins `reads`, selects that project. A pushed range whose base is all zeros (a new
    branch) or that shares no history with its base counts every path as changed; a symmetric
    range (`...`) is refused.
  - An exception accepts only the findings of its own scope (the root file also accepts
    repository-scope findings); repository-scope rules are excepted in the root file, project-scope
    rules in the project's. Exceptions and baseline entries are due by (scope, rule): the plugin
    runtime fills `RulesRan` with the rules it ran to completion.
  - A layer's `"<plugin>/*"` level overrides the levels earlier layers gave that plugin's rules.
  - `[workspace] ignore` removes directories from project discovery, and a `gate.toml` below an
    ignored directory is not read.
  - The leak scan of the whole worktree reports pre-existing lines in `README.md` and `PROVENANCE.md`
    (outside this task's Touches, T-011's files); the same scan over this task's paths prints nothing.

## Resume point

2026-11-16T20:45:31+00:00: in review, after the one fix round on the review (B1, M1-M3,
m1-m6, n1-n5 all addressed); rebased on `main`. Gate commands, run in the worktree:
`cargo fmt --all --check` 0, `cargo clippy --locked --all-targets -- -D warnings` 0,
`cargo test --locked` 0 (cli 12 passed after removing three wiring-only tests, host 100 passed),
`cargo deny --locked check` 0. `cargo build --release --locked` was not rerun in this round.
Next: the lead's decision.

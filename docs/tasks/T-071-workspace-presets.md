# T-071: A workspace root extends presets for its repository-scope rules

| Field | Value |
| --- | --- |
| Status | design approved by the lead (2026-11-20) as drafted; code in progress |
| Release | v0.3.0 (configuration; no plugin interface change) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | `origin/main` at `cee6491` (batches 1 and 2) |
| Touches | `crates/gate-host/src/config` (`[workspace] extends`, its origin label, the parser hint), `crates/gate-host/src/workspace.rs` (where it is allowed), `crates/gate-host/src/plugins.rs` (the check that an `extends` entry turns something on), `crates/gate-runtime/tests/probe` (a test preset of both scopes), `crates/gate/tests/plugins.rs`, `crates/gate-host/tests/host/config.rs`, `docs/guide/configuration.md`, `docs/guide/plugins.md`, `plugins/git/rules/commit-message.md`, the root `CHANGELOG.md` |

## Goal

Found by T-066. In a monorepo whose root is not a project, the root `gate.toml` may not hold
`[project]` (`check_workspace_root`), so it has no `extends`. Profiles reach projects only, and a
repository-scope rule set in a profile or a member project is a configuration error. So the
repository-scope rules of a preset (`git/recommended`: `branch-name`, `commit-message`;
`repository/recommended` of T-066; `agents/no-claude-md`, repository-scope from T-062, from
`agents/recommended` of T-069) cannot be turned on by a preset in a monorepo, only one by one in the
root `[rules]`.

## What main does today (read at `5da3272`, rechecked at `cee6491`)

- The repository's settings are the root file's own (`Workspace::repository` = `root.own()`): its
  `[plugins]`, its `[rules]`, and the `[project] extends` of the root file when the root is a
  project.
- Each run filters a preset's rules by the scope it runs in (`enabled_rules` in `plugins.rs`): the
  repository call takes the presets' repository-scope rules, a project's call their project-scope
  rules. A rule of the other scope is skipped silently. So a single-project root that extends
  `git/recommended` in `[project]` already runs `branch-name` and `commit-message` at repository
  scope, and a member project that extends a preset with a repository-scope rule skips that rule.
- The level a preset gives yields to `[rules]`: the root's own `[rules]` entry (whole rule, then
  aspect, then family) wins over the presets' (`level` in `plugins.rs`).

So the runtime already applies root presets correctly. The gap is only that a workspace root has
nowhere to write them.

## 1. Where: `[workspace] extends`

```toml
[gate]
version = "0.3.0"

[workspace]
projects = ["apps/*", "packages/*"]
extends = ["git/recommended", "agents/recommended"]

[plugins]
git = { source = "ElsiKora/Gate:git@0.3.0" }
agents = { source = "ElsiKora/Gate:agents@0.3.0" }

[rules]
"git/commit-message" = { level = "error", options = { header-max-length = 72 } }
```

- `extends` under `[workspace]`, a list of `"<plugin>/<preset>"`, as `[project] extends` and a
  profile's `extends` are. Not a top-level `extends`: a single-project root would then have two
  places for the same thing (`[project] extends` and the top level). `[workspace]` is the section
  only a root that holds projects has, so the key sits where the reader looks for the root's role.
- Allowed only when the root is not a project. When the root is a project (no `[workspace]`, or a
  `[workspace] projects` that lists `"."`), `[project] extends` already feeds the repository scope,
  and `[workspace] extends` is a configuration error: `gate.toml: [workspace] extends: the root is
  a project, so its presets go in [project] extends, which applies them to the repository's rules
  too`. One way for each kind of root.
- Its entries join the repository's settings (`root.own().extends`), in file order, deduplicated as
  layers already are. The origin label is `[workspace] extends`, so `gate list` shows
  `extends git/recommended  gate.toml:5 [workspace] extends`.
- The root's own `[rules]` overrides them, as for every preset: nothing new, the existing order in
  `level` holds (a `[rules]` level of the whole rule applies to every aspect, the preset's aspect
  levels only where `[rules]` sets none).
- A preset of a plugin the root does not declare is the error every `extends` gives today
  (`check_settings`).
- The parser's hint for an unknown `extends` names the three places: `[project] extends`,
  `[workspace] extends` (a root that is not a project) and a `[[profiles]]` entry.

## 2. A project-scope rule in a root preset: skipped, unless the entry turns on nothing

- A rule of a root preset whose scope is project is skipped at the root, without a message: the
  runtime already does so. Presets are written per plugin, not per scope: once T-062 and T-069 are
  in, `agents/recommended` holds `no-claude-md` (repository) and eight project-scope rules, so a
  root that extends it gets `no-claude-md` and a profile that extends it gets the other eight, each
  without noise. Refusing a
  mixed preset at the root would force every plugin to ship one preset per scope, and
  `git/recommended`-style presets that are all repository-scope would be the only ones a root could
  name.
- An `extends` entry that turns on no rule where it applies is a configuration error, wherever it is
  written. With `[workspace] extends`, `extends = ["typescript/recommended"]` at a workspace root
  would silently do nothing, as a member project's `extends = ["git/recommended"]` does today: in
  both, the writer expected rules to run and none do. The check, per entry (by its origin), across
  every scope the entry reaches:
  - `[workspace] extends`: the repository scope;
  - a profile's `extends` and a member project's `[project] extends`: project scope;
  - a root project's `[project] extends`: both scopes, so `git/recommended` there passes through its
    repository rules and `typescript/recommended` through its project rules.
  The error: `gate.toml:5: [workspace] extends "typescript/recommended": the preset turns on no
  repository-scope rule, and the root runs only those; extend it in a [[profiles]] entry for the
  projects`, and for a project or profile: `... turns on no project-scope rule; extend it in the
  root's [workspace] extends`. A preset that names a rule of the scope at `off` still counts: it
  says something about that rule.
- Why one check everywhere rather than only at the root: the same mistake has the same message, and
  a member project that extends a repository-only preset is that mistake in the other direction. It
  is checked where presets are already checked (`check_presets`, which reads every declared plugin's
  presets whether or not the run runs it), so a `--rule` filter does not hide it.

## 3. A single-project repository: unchanged

The root is the project, `[project] extends` applies to both scopes, and `[workspace] extends` is
refused there (section 1). No second way, and the only new behaviour for such a root is the check of
section 2, which a preset that turns on anything at either scope passes.

## 4. File exceptions and rule decisions at a non-project root: out, a task of its own

Out of T-071. What already works: the root's `gate.exceptions.toml` accepts repository-scope
findings at a workspace root (`ScopedException::is_root`), and the root's `[rules]` may name a
`decision` (T-048, which renames `[exception-policy]` to `[decision-policy]`). What is missing is
that no ledger rule checks them: the ledger rules run in a project's call, a path relative to that
project cannot name the root file, and T-059 puts the root file's decisions in no call without a
root project. Closing it needs a repository
scope way to read the root's exceptions and rule decisions against the index (a repository-scope
ledger rule, or the request carrying them to the project that holds the ledger), which is a plugin
and request change of its own with its own tests, while T-071 changes only where the root's presets
are written. I propose it as T-072, written after T-059 merges, so it builds on the request T-059
adds.

## Tests

In `crates/gate/tests/plugins.rs` with the built `git` component and the runtime's probe component,
and in `crates/gate-host/tests/host/config.rs`. `agents/recommended` is not on `main` before T-069,
so the mixed-scope case uses a probe preset, `probe/both`, of one repository-scope and one
project-scope probe rule:

- A monorepo root (`[workspace] projects = ["apps/*"]`, one project) with
  `[workspace] extends = ["git/recommended"]` and a commit message out of form runs
  `git/branch-name` and `git/commit-message` at `error`, `header-min-length` at `warning`, and exits
  1.
- The root's `[rules]` wins: `"git/commit-message" = "note"` makes every aspect a note and the run
  exits 0; an aspect level there wins over both.
- A mixed preset from the root: `[workspace] extends = ["probe/both"]` runs the repository-scope
  probe rule at the root and not the project-scope one, with no error or warning; a profile that
  extends it runs the project-scope rule in each matching project and not the other.
- An entry that turns on nothing: `[workspace] extends = ["typescript/recommended"]` is a
  configuration error (exit 2) naming the entry and the profile fix; a member project's
  `extends = ["git/recommended"]` is one naming `[workspace] extends`.
- `[workspace] extends` in a root that is a project (`[workspace] projects = [".", "apps/*"]`) is a
  configuration error naming `[project] extends`.
- `gate list` shows the entry with the origin `[workspace] extends`.

## Docs

- `docs/guide/configuration.md`: `[workspace] extends` in the `[workspace]` table; where presets go
  for each kind of root; the rule of section 2.
- `docs/guide/plugins.md`: the `repository` section, which today says a monorepo root extends no
  preset, shows `[workspace] extends`; the Presets section names the three places.
- `plugins/git/rules/commit-message.md`: the workspace-root paragraph, which today writes the levels
  in `[rules]` because a workspace root "holds no `extends`", shows `[workspace] extends`.
- Root `CHANGELOG.md`: `# 0.3.0 (unreleased)`, a Features entry, and a Breaking Changes entry for
  the section 2 check: an `extends` that turned on nothing used to pass. It says how to move: a
  member project's or a profile's `extends` of a repository-only preset such as `git/recommended`
  moves to the root's `[workspace] extends`.

## The lead's rulings (2026-11-20)

The design is approved as drafted: `[workspace] extends`, allowed only where the root is not a
project, and a configuration error naming `[project] extends` at a root project, with no top-level
`extends` (1); a root preset's project-scope rules skipped silently, and the error for an entry that
turns on no rule where it applies checked everywhere, per entry, in `check_presets`, the member
project's `extends = ["git/recommended"]` included, with a Breaking Changes entry that says how to
move it (2); the single-project repository unchanged apart from (2) (3); the root's exceptions and
rule decisions in T-072 (4). The test plan is accepted, `probe/both` included.

## Follow-ups

- **T-072: the root's exceptions and rule decisions reach the ledger without a root project.** In a
  repository whose top is not a project, the root `gate.exceptions.toml` and the `decision` of the
  root `[rules]` entries suppress and relax findings, but no ledger rule checks them against the
  decision index: the ledger rules run in a project's call, a path relative to that project cannot
  name the root file, and T-059 puts the root file's decisions in no call without a root project.
  T-072 gives them a repository-scope reader, either a repository-scope ledger rule that reads the
  index from the repository root or the request carrying the root's entries to the project that
  holds the ledger, with tests for exceptions and rule decisions alike. It is written after T-059
  merges, on the request T-059 adds.

## Done when

- The lead accepts sections 1 to 4.
- One lane round on the pushed tip, its own target directory and its logs kept per ticket: fmt,
  clippy, test, deny and the release build exit 0; the log shows each changed crate compiled from
  this task's round checkout and lists each new test by name; each new test fails with its change
  reverted.

## Resume point

2026-11-20T16:26:20+00:00: the design is approved and committed on a branch from `origin/main` at
`cee6491`; the code is next.

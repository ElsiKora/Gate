# Changelog of the agents plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:agents@<version>`.

# 0.3.0 (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Breaking Changes

- `agents/no-claude-md` is a repository-scope rule: it runs once, sees every tracked path, and
  reports every `CLAUDE.md` in any case, a root one included when the workspace lists only nested
  projects. Set it in the root `gate.toml`'s `[rules]`; a project's file may no longer set it.
  Delete any `CLAUDE.md` that only imports `AGENTS.md`, and move the instructions of any other into
  `AGENTS.md` or a rule file.
- An enabled rule fails when its input is missing, instead of reporting nothing: the rules that
  read the rule files (`agents/rule-registry`, `agents/rule-ids`, `agents/rule-file-size`,
  `agents/rule-enforcement`, `agents/rule-owner`, `agents/rule-references`) report an empty
  `<root>/rules` as their aspect `input`, and `agents/instruction-references` reports a missing
  `entry` the same way. Add the agent documents, or set `levels = { input = "off" }` with a
  decision.
- The `agents-block` generator reports an `entry` that is not tracked, or a `<root>/rules` with no
  rule file, as `gate/sync#agents/agents-block` in `gate sync` and `gate sync --check`; before, it
  wrote nothing and said nothing. A project that does not want the block removes the generator from
  `[generate]`.

### Features

- The presets `agents/recommended`, all nine rules at `error`, and `agents/adoption`, the same at
  `warning`.
- `agents/agents-block` seeds the entry (`AGENTS.md`) when it is not tracked: a title, one line, and
  the managed block filled from the rule files. `gate sync` writes the seed only where nothing
  exists. An empty `<root>/rules` stays a finding of `gate/sync#agents/agents-block`; rule files are not
  seeded.

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-agents-v0.1.0...plugin-agents-v0.2.0) (2026-11-19)

Built for the plugin interface `gate:plugin@0.2.0`; it loads only in a 0.2.0 core.

### Changed

- `agents/rule-ids`, `rule-registry`, `skills`, `instruction-references`, `rule-enforcement`,
  `adapters` and `rule-file-size` give each finding whose message names a line, a path or an
  option value a stable key (what it is about, never a line or column), so an edit above a finding
  or a reworded message keeps its fingerprint. These findings' fingerprints change once, with this
  release; `gate baseline prune` re-keys their baseline entries when the repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))
- A wrong option ends the run as a configuration error (exit 2, `configuration error`), not as
  an environment error. A file the rule cannot read stays an environment error. ([eaa4f37](https://github.com/ElsiKora/Gate/commit/eaa4f3770f96227855b6a1b7c438d966d9dbf24a))

# 0.1.0

- `agents/rule-registry`, `agents/rule-ids`, `agents/rule-enforcement`, `agents/rule-references`:
  the rule-file format, its ids, enforcement claims and references.
- `agents/instruction-references`: backticked paths and commands in the agent instructions exist;
  in a project, a path cited from the repository root resolves among the granted paths.
- `agents/skills`: skills follow the Agent Skills specification.
- `agents/rule-file-size`, `agents/rule-owner`, `agents/no-claude-md`.
- Generators `agents/adapters` (Claude Code adapters and skill copies) and `agents/agents-block`
  (the rule-file table of `AGENTS.md`).

# Changelog of the agents plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:agents@<version>`.

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

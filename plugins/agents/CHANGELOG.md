# Changelog of the agents plugin

The plugin is versioned on its own; its source is `ElsiKora/Gate:agents@<version>`.

## Unreleased

### Changed

- `agents/rule-ids`, `rule-registry`, `skills`, `instruction-references`, `rule-enforcement`,
  `adapters` and `rule-file-size` give each finding whose message names a line, a path or an
  option value a stable key (what it is about, never a line or column), so an edit above a finding
  or a reworded message keeps its fingerprint. These findings' fingerprints change once, with this
  release; `gate baseline prune` re-keys their baseline entries when the repository upgrades.
- A wrong option ends the run as a configuration error (exit 2, `configuration error`), not as
  an environment error. A file the rule cannot read stays an environment error.

# 0.1.0

- `agents/rule-registry`, `agents/rule-ids`, `agents/rule-enforcement`, `agents/rule-references`:
  the rule-file format, its ids, enforcement claims and references.
- `agents/instruction-references`: backticked paths and commands in the agent instructions exist;
  in a project, a path cited from the repository root resolves among the granted paths.
- `agents/skills`: skills follow the Agent Skills specification.
- `agents/rule-file-size`, `agents/rule-owner`, `agents/no-claude-md`.
- Generators `agents/adapters` (Claude Code adapters and skill copies) and `agents/agents-block`
  (the rule-file table of `AGENTS.md`).

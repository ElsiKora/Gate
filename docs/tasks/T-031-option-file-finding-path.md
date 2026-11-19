# T-031: A finding in a nested project's option file names its path once

| Field | Value |
| --- | --- |
| Status | planned |
| Release | v0.2.0 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | the v0.1.0 root |
| Touches | the host's mapping of finding locations into the repository, its tests |

## Goal

In a project below the repository root (`apps/server/gate.toml`), a finding located in one of the
project's option files names `apps/server/apps/server/gate/options/<plugin>/<rule>.json`: the
project's root is joined twice. It must name `apps/server/gate/options/<plugin>/<rule>.json`, as every
other finding of that project names its path once. Seen with rc.1 on the first consumer's converted
tree, in two findings of `nestjs/controller-owners` and `nestjs/field-inventory`.

## Scope

1. Find where an option file's path, already repository-relative, is mapped from the project again.
2. Tests: a nested project whose rule reports on its option file names the path once, in the text, JSON
   and SARIF reports; the same at the repository root.

## Done when

- The tests above, each failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: planned; branch `task/T-031-option-file-finding-path`.

# T-031: A finding in a nested project's option file names its path once

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.2.0 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | the v0.1.0 root |
| Touches | `crates/gate-runtime/src/runtime.rs` (the check request), `crates/gate-host/src/plugins.rs` (a call's read grant), the WIT and SDK docs of `options-file`, the probe plugin, `crates/gate/tests/plugins.rs` |

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
3. The WIT promises a rule can read its option file (lead, 2026-11-19): the host adds each rule's
   own option file to its call's read grant, the file alone and not its directory, so an option file
   of a root `[[profiles]]` entry, outside the project, is readable too. Test: such a file, read by
   a rule of a nested project and reported at its repository path.

## Cause and change

The host reads a rule's `options-file` and keeps the repository path of the file
(`apps/server/gate/options/nestjs/controller-owners.json`), and the runtime handed that path to the
plugin as `options-file`. A rule addresses every path relative to its scope's root, so a finding it
reported at that path was mapped from the project root again, and reading it through the tree
missed the file. The runtime now gives the plugin the option file relative to the call's grant
(`Grant::to_plugin`), as every other path: the finding is mapped back once, and the rule can read the
file. The WIT and the SDK's `RuleRun` say so. The host adds the option files of the rules a call
runs to that call's grant, each file alone (`grant` in `crates/gate-host/src/plugins.rs`).

Tests (`crates/gate/tests/plugins.rs`), through the probe's `read` rule, whose new `at_options_file`
reports a finding at its option file with what reading it answers:

- `a_finding_in_a_nested_projects_option_file_names_its_path_once`: project `apps/one`, option file
  in its own `gate.toml`; the rule reads the file, and the text, JSON and SARIF reports name
  `apps/one/gate/options/probe/read.json` and never `apps/one/apps/one`. It fails on the runtime
  before the change.
- `a_finding_in_the_option_file_of_a_single_project_repository_names_its_path`: the same at the
  repository root. The root's scope root is empty, so this case was right before the change too; the
  test keeps it right.
- `a_rule_reads_a_root_profiles_option_file_and_reports_at_its_repository_path`: the root's
  `[[profiles]]` gives a rule of `apps/one` the option file `gate/options/probe/read.json`; the rule
  reads it as `../../gate/options/probe/read.json`, cannot read its neighbour `other.json` (the file
  is granted, not its directory), and its finding names `gate/options/probe/read.json`. It fails
  before either change.

## Done when

- The tests above, each failing before the change.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: branch `task/T-031-option-file-finding-path` on main `59fb163`, the first public release.
Change, tests and the option-file grant written; next: the gates, with the tests run once on the
runtime before the change and once on the host before the grant, then the push and the review.

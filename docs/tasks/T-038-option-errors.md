# T-038: A rule's option error is a configuration error

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | T-036 (`task/T-036-commit-message-options` at `c8afefb`, for `plugins/git/src/javascript.rs`); merges right after T-036 |
| Touches | `wit/gate.wit`, `crates/gate-sdk`, `crates/gate-runtime`, `crates/gate-host`, every plugin's option-validation sites, `CHANGELOG.md` |

## Goal

A plugin can tell the core that the configuration it was given is wrong, apart from an environment
failure. A bad rule or generator option then ends the run as a configuration error (exit 2, the
label `configuration error`), as a bad `gate.toml` key does, and not as an environment error.

## Scope

Step one (pushed as soon as it compiles):

1. WIT: `check` and `generate` answer `result<_, plugin-error>`, with
   `variant plugin-error { configuration(string), environment(string) }`.
2. SDK: `Error::config` beside `Error::new`; options that do not match a rule's or generator's
   options type, and `[plugins]` options or `[languages]` settings that do not match, are
   configuration errors inside the SDK. The kind passes through the `rule ... failed` wrapping.
3. Runtime: a new `ErrorKind::Configuration` for an answered `configuration`; it recurs for the same
   input, so it never marks a failure as one that may pass. Host: a call that fails with it is a
   configuration error.
4. The git plugin swept: only option-validation sites move to `Error::config`; read, parse and git
   failures stay environment errors. `git/branch-name` reads its patterns through
   `plugins/git/src/javascript.rs`, as `git/commit-message` does, and `ascii_classes` goes, with a
   test per silent case `git/branch-name` had and a line on its rule page.
5. Tests: a bad option in two plugins gives exit 2 and the configuration-error label; a missing
   plugin file keeps its environment-error label.

Step two: the other plugins swept site by site, by the same rule.

`CHANGELOG.md`: `Changed`, the WIT and the SDK. No parsing of message conventions.

## Moved sites

Each `Error::new` site moved to `Error::config`, by file:line at the commit that moves it.

Step one:

- SDK: `crates/gate-sdk/src/plugin.rs:258` (options that do not match the options type),
  `crates/gate-sdk/src/rule.rs:209` (`[plugins]` options), `crates/gate-sdk/src/rule.rs:221`
  (`[languages]` settings).
- `plugins/git/src/settings.rs`: 50 (the `[plugins.git]` options do not match), 55 (`main` empty),
  66 (`activation` not a full id), 76 (`release-subject` without a group), 85 (`branch-length`
  inverted), 99 (no `activation`), 220 (a required key missing), 232 (a pattern that does not
  compile), 250 and 256 (globs).
- `plugins/git/src/forms.rs`: 45, 198, 208, 214, 228, 238, 246 (forms that cannot work).
- `plugins/git/src/commit_message.rs`: 250 and 256 (`header-pattern` and `ignores` refused or not
  compiling), 270 (no `type` group).
- `plugins/git/src/branch_name.rs`: 166 (`min-length` above `max-length`), 187 (template and
  pattern), 287 (a template with nothing to match), 426 and 432 (a pattern refused or not
  compiling).
- `examples/plugin-hello/src/lib.rs:34` (`marker` empty), the second plugin of the end-to-end test.

Kept as environment errors: `plugins/git/src/history.rs:76` (no main reference exists) and
`plugins/git/src/history.rs:83` (the activation commit is not an ancestor of main), since a fetch of
the full history may fix either; the SDK's `are not JSON` and `has no rule` (what the core sent,
not what the user wrote).

Step two (ci, agents, markdown, typescript):

- `plugins/ci/src/workflows.rs:71` (`workflows` not a glob).
- `plugins/agents/src/rule_file_size.rs:53` (`warn-lines` above `max-lines`),
  `plugins/agents/src/agents_block.rs:139` (`entry` not a plain path),
  `plugins/agents/src/instruction_references.rs:85` (`command-pattern` without a group).
- `plugins/markdown/src/frontmatter.rs:65` (`required-keys` empty),
  `plugins/markdown/src/required_sections.rs:118` (a `sections` path pattern that does not
  compile).
- `plugins/typescript/src/barrel_imports.rs:77` (`barrel-name` not a file name), 146 and 155
  (globs), `plugins/typescript/src/parse.rs:58` (`extensions` naming one oxc does not parse).

Kept as environment errors in step two: a file that is not UTF-8, YAML or JavaScript
(`plugins/ci/src/workflows.rs:84` and 89, `plugins/agents/src/load.rs:135`,
`plugins/markdown/src/pages.rs:50`, `plugins/typescript/src/parse.rs:82` and 84), the language
library's read and resolve failures passed through (`plugins/typescript/src/barrel_imports.rs:275`,
283, 426), and `plugins/ci/src/actions_pinned.rs:33` (built-in patterns, a bug). The language
library's own `Error` has no kind, so `languages/typescript/src/lib.rs:182` (a configured
`tsconfig` that is not tracked) stays an environment error until that type carries one.

## Done when

- fmt, clippy, test (`--no-fail-fast`), deny and build exit 0 in one lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-038-option-errors` from T-036's tip `c8afefb`; this file first. No round
until T-036's ticket 187 is green.
2026-11-19: step one written: `plugin-error` in the WIT, `Error::config` in the SDK,
`ErrorKind::Configuration` in the runtime, the host's label; the git plugin and the hello example
swept; `git/branch-name` on `javascript.rs` with `ascii_classes` gone, eleven per-case tests and a
line on its page; the end-to-end test of hello's and git's bad options (exit 2, `configuration
error`) and a missing `--plugin` file (`environment error`). Waiting for T-036's ticket 187 before
a round.
2026-11-19: step two for ci, agents, markdown and typescript written (sites above), with a
`Changed` entry in each plugin's changelog; nestjs and ledger wait for the lead's split.

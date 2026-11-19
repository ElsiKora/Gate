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

## Done when

- fmt, clippy, test (`--no-fail-fast`), deny and build exit 0 in one lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-038-option-errors` from T-036's tip `c8afefb`; this file first. No round
until T-036's ticket 187 is green.

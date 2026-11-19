# T-039: The release branch compiles again

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | `release/v0.2.0` at `8157571`; merges first, before T-036 |
| Touches | `crates/gate-runtime/src/state.rs`, `crates/gate-runtime/src/read_set.rs`, one end-to-end test |

## Goal

`release/v0.2.0` compiles. The commit-message and hook questions that T-029 added to the plugin
interface take their place in the result cache that T-025 added, so that a stored result is never
reused for a different message or a different hook state.

## Scope

1. `host_call` kinds `git.message` and `git.hook` in `state.rs`.
2. `GitQuery::Message` and `GitQuery::Hook` in `read_set.rs`: `depends_on` asks the question again,
   `lasting` is false for both, `to_words` and `from_words` write and read `message` and `hook`.
3. One test: with the result cache warm, a `--commit-message` check of a second, different message
   runs the rule again and gives the second message's verdict; a change of the hook state likewise,
   if cheap.

## Done when

- fmt, clippy and test (`--no-fail-fast`) exit 0 in one lane round on the pushed tip.
- Do not edit docs/plan.md.

## Resume point

2026-11-19: branch `task/T-039-release-compiles` from `release/v0.2.0` at `8157571`; this file first.
2026-11-19: written. `git.message` and `git.hook` are host-call kinds; `GitQuery::Message` and
`GitQuery::Hook` ask again in `depends_on`, are never lasting, and read back from their words (the
unit test's list holds both). Test `a_warm_result_cache_reruns_the_rule_for_another_message_or_hook_state`:
one message file whose text changes runs, then comes from the cache, then a bad message runs the
rule again and fails; `git/branch-name` on main passes outside a hook, is cached, and fails when
`--hook` is given. No native cargo run; one lane round runs fmt, clippy and test.
2026-11-19: round 179 (7e6ac15): fmt exit 0; clippy failed on `gate-host/src/plugins.rs` `run`, 109
lines over the limit of 100, another miss of the same merge (T-029's message and hook fields in the
`GitHistory` literal). The literal moves into `GitHistory::new`, and `run` is 97 lines.

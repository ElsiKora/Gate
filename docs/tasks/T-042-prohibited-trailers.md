# T-042: prohibited-trailers of git/commit-message

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.1 |
| Writer | "Gate 002" |
| Reviewer | the lead |
| Depends on | `main` at `89c42c7` (v0.2.0) |
| Touches | `plugins/git/src/commit_message.rs`, its tests, `plugins/git/rules/commit-message.md`, `plugins/git/CHANGELOG.md` |

## Goal

A repository that refuses some commit trailers (an AI co-author trailer, for example, which this
repository's own `AGENTS.md` refuses) can say so as an option of `git/commit-message`, instead of a
shell step. commitlint has `trailer-exists`, which requires a trailer, and nothing that refuses one.

## Scope

1. `prohibited-trailers`: a list of regular expressions, read as the plugin's other patterns are
   read (JavaScript without flags). Each is searched in each trailer of the message's trailer
   block, written `Token: value`, and every trailer one matches is one finding of the aspect
   `prohibited-trailers` that names the trailer. Empty by default, so nothing changes for a
   repository that does not set it.
2. The trailer block is found as `git interpret-trailers` finds it: the message's last paragraph,
   never the first (the title), with comment lines and what follows the scissors line left out; it
   is a trailer block when every line of it is a trailer (`Token: value`, the token of letters,
   digits and `-`) or continues one (starts with whitespace), or when it holds a line git writes
   itself (`Signed-off-by: `, `(cherry picked from commit `) and at least a quarter of its lines
   are trailers. A trailer is matched unfolded: token, `: `, the value with its continuation lines
   joined by a space.
3. The same text anywhere else in the message (the body, a paragraph above the last) is not a
   trailer and is not refused.
4. A pattern that does not compile or holds a construct the translation does not read is a
   configuration error (exit 2) that names `prohibited-trailers`.
5. The rule page and the plugin's changelog (`Unreleased`, `Added`), with synthetic examples.

## Done when

- A message whose trailer block holds a prohibited co-author trailer fails with one
  `prohibited-trailers` finding that names the trailer.
- The same line in the body, not in the trailer block, passes.
- A trailer no pattern matches passes.
- A pattern that cannot work is a configuration error naming `prohibited-trailers`.
- `rustfmt --edition 2024 --check` on the changed `.rs` files exits 0 before the commit.
- fmt, clippy, test (`--no-fail-fast`), deny, build and the git plugin's wasm build exit 0 in one
  lane round on the pushed tip.
- Do not edit docs/plan.md. Turning the option on for this repository is a follow-up.

## Resume point

2026-11-19: branch `task/T-042-prohibited-trailers` from `origin/main` at `89c42c7`; this file
first.
2026-11-19: written. `prohibited-trailers` in `plugins/git/src/commit_message.rs`: the patterns
compile with the rule's other options (a bad one is a configuration error naming the option), and
`trailers()` finds the block as git's `find_trailer_block_start` does (last paragraph after the
title, all trailers or continuation lines, or a git-written line and a quarter trailers; comment
lines and the scissors section out) and unfolds each trailer to `Token: value`. One finding per
refused trailer, aspect `prohibited-trailers` (the 22nd), target and key the trailer. Decision to
confirm with the lead: an ignored message (merge, `fixup!`, the `ignores`) is still checked for
trailers, since a trailer stays in history whatever the header says. Tests: a unit test of the
block, and the lead's four (refused co-author, the same text outside the block, a trailer no
pattern matches, a pattern that cannot work). Rule page (aspect, option, section, difference) and
the plugin changelog under `Unreleased`. rustfmt --edition 2024 --check on both files: exit 0.
2026-11-19: Proof of `2dc41a1`, lane ticket 206 (17:40:03 to 17:56:54 UTC, log
`T-042/lane/run-1.log` outside the repository): fmt 0, clippy 0, `cargo test --locked --workspace
--no-fail-fast` 0 (1018 passed, 0 failed, 7 ignored, summed from its `test result:` lines), deny 0
(advisories, bans, licenses, sources ok), `cargo build --release --locked` 0, and the wasm32-wasip2
release build of each of the eight plugins 0. The lead confirmed that an ignored message is still
checked for trailers.
2026-11-19: the review's fix round. The rule page and the changelog say the option reads the
message being written (the `commit-msg` hook's), so a commit made without the hook is not checked.
A trailing old-style `Conflicts:` block (and the trailing comment and empty lines) is dropped before
the trailer block is found, as git's `ignored_log_message_bytes` drops it. An empty pattern is a
configuration error. The option's example is `^[Cc]o-[Aa]uthored-[Bb]y: `. The comment-line case
moved to the refusing test, where it fails without the comment filter: a `#` line inside the block
would make it prose. A `#` line can never be a trailer, so no passing case depends on that filter.
rustfmt --edition 2024 --check on the changed files: exit 0.

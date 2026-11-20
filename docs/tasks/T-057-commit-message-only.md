# T-057: a `--commit-message` check checks only the message

| Field | Value |
| --- | --- |
| Status | design approved by the lead; tests and the change pushed, gates wait for T-047 |
| Release | v0.3.0 (it changes what an existing `commit-msg` hook checks) |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | T-047 (a check loads only the plugins it needs) |
| Touches | `wit/gate.wit` (`gate:plugin@0.3.0`), `crates/gate-sdk` (the rule declaration, `Run::message`), `crates/gate-host/src/{lib.rs,plugins.rs}`, `crates/gate/src/main.rs`, `plugins/git` (the rules that read the message); `docs/guide/getting-started.md`, `docs/guide/configuration.md`, `CHANGELOG.md` (Changed) |

## Goal

The hook the guide shows is `commit-msg = ["check --commit-message"]`. With no `--staged` and no
`--rule`, that check is `ChangeRequest::All` on the work tree (`check` in
`crates/gate/src/main.rs`): it checks the whole work tree and the message, and its summary says
`tree, commit message`. The tree is checked twice per commit, by `pre-commit` on the index and by
`commit-msg` on the work tree. On a cold cache the second check compiles every plugin a scope
declares, not only `git`. The lead decided on 2026-11-19: a `--commit-message` check runs only
the rules that read the message being written. The tree stays with `pre-commit`, `pre-push` and
CI.

## Design

1. **A rule declares that it reads the message.** The lead decided on 2026-11-19:
   - A constant beside `READS_HISTORY`, `const READS_MESSAGE: bool = false`, carried in
     `rule-info` as `reads-message: bool`. `Rule::NEEDS` keeps its one meaning: keys the plugin's
     options must hold.
   - It joins `gate:plugin@0.3.0`, the interface `main` already moved to for v0.3.0 (`seed`,
     `plugin-options`, `tree.size`); a 0.3.0 core loads only plugins built for 0.3.0. Plugins are
     versioned apart from the core, so the load error names the interface, not a core version.
   - `Run::message` answers only to a rule that declares it, and `Run::git` only to a rule that
     declares `READS_HISTORY` (the review round moved the message from `Git::message` to
     `Run::message`, so a rule that declares only `READS_MESSAGE` reaches no history and cannot
     escape `reads-history` in `gate sync`'s fetch depth). A rule that reads the message without
     the declaration gets an error that names `READS_MESSAGE`, and one that reads history without
     `READS_HISTORY` an error that names that, so the selection below cannot miss a rule that
     reads the message.
   - A project-scope rule that declares it is refused when its plugin describes itself, because
     the message belongs to the repository.
2. **What the run selects.** A check with `--commit-message` runs only the enabled rules that
   declare the message, in the repository scope. `--rule` narrows that further, as anywhere
   else. The core's own rules do not run, except `gate/budget`: the hook's time budget holds in a
   message run as in any other (the review round kept it). The summary says `commit message`.
3. **What it loads.** T-047's set, narrowed to the plugins that declare such a rule. Which rules
   those are is known only from each plugin's `describe`. Recommendation: the run loads the
   plugins T-047 would load and runs only the message rules.
   - After `gate sync`, every plugin is in the module cache (T-047's recommendation). A warm load
     is cheap: 87 ms for all seven plugins in the warm row of lane ticket 205.
   - The alternative is a stored copy of each plugin's declaration under (plugin sha256, core
     version), written whenever a plugin loads. A cold message check would then compile only
     `git`. It is one more cache kind, so it waits until a measure shows the warm load matters.
4. **One way.** `--commit-message` with `--staged`, `--changed`, `--push` or `--rev` is a usage
   error. The message says the tree is checked by `pre-commit`, `pre-push` or CI. There is no
   option to check the tree and the message in one run.
5. **The `git` rules.** `git/commit-message` and `git/no-ci-skip` declare the message.
   Both also keep `READS_HISTORY`, because both check pushed commits in `pre-push`
   (`git/commit-message` since `main`'s commit-message push range; the review round's reading
   that it declares only `READS_MESSAGE` held before that change and no longer does). Each reads
   the message through `Run::message` and the pushed commits through `Run::git`. A rule that reads
   only the branch, such as `git/branch-name`, does not declare it, and so does not run in
   `commit-msg`.
6. **Documents.** The getting-started page (the `commit-msg` paragraph and its summary line) and
   the configuration page (the `[hooks]` paragraph) say what the hook checks. `CHANGELOG.md`
   lists it under Changed: an existing `commit-msg` hook no longer checks the tree.

## Tests

Each test fails before its change. They use synthetic plugins and trees.

- In a repository with a tree rule that fails and a message rule that passes,
  `check --commit-message <file>` exits 0 and reports only the message rule. Before the change,
  it fails on the tree rule.
- A rule that calls `Run::message` without the declaration gets the error that names it, with or
  without `READS_HISTORY`; a rule that declares only `READS_MESSAGE` and calls `Run::git` gets the
  error that names `READS_HISTORY`.
- A project-scope rule that declares the message fails its plugin's load as broken.
- A component built for `gate:plugin@0.2.0` does not load under the 0.3.0 core, and the error
  says to use the plugin's 0.3.0 release.
- `check --commit-message <file> --staged` is a usage error that names `pre-commit`.
- The summary of a message run is `gate check (commit message): ...`.

## Open items

- **The one interface bump.** Any other change that needs the interface must land in the same
  `gate:plugin@0.3.0` bump, before v0.3.0 is tagged. Carrying rule-entry decisions to plugins is
  one. Each such change is named here, or in its own task file that points here, before this
  task's interface commit merges.

## Done when

- The tests above, each failing before its change.
- The consumer's cold `commit-msg` row, before and after, with the command and the lane ticket.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-20T02:23:16+00:00:
- Branched from main `73d33e7`, then rebased onto the T-047 tip `1fb98c0`, because the load set
  of item 3 is T-047's. It is rebased again onto main when T-047 merges.
- The lead approved the design: item 1 is `READS_MESSAGE` and `gate:plugin@0.3.0`, and items 2
  to 6 stand as written. The `CHANGELOG.md` entries are under Unreleased, Changed.
- `e77337c` holds the tests, written before the change.
- `6ffb35b` holds the change: the interface, the SDK, the runtime gates and the project-scope
  refusal, the host's message-only selection, the usage error, the `git` rules and the documents.
- Nothing is compiled yet. The round runs on the lane only after T-047's gates are green, so a
  T-047 fault does not cost two rounds. The round runs the tests on `e77337c` first, where each is
  expected to fail, then the gates on the tip.
- T-047's redesign went green in lane ticket 256 on `65e5a55`. This branch is rebased onto its
  tip `f7269b4` (no conflict): the tests are now `5bf5500`, the change `fbf6e35`. The stored
  declaration of a rule (`plugins/declarations.rs`, `StoredRule`) gains `reads_message`, so a run
  that answers from the cache still knows which rules read the message; its entry name already
  depends on the interface, so an entry written for `gate:plugin@0.2.0` is never read.
- Rounds: ticket 264 ran the six new tests on `5bf5500` (each exit 101, as expected) and stopped
  at clippy (`too_many_lines` on `Runtime::load`, 111 of 100), fixed in `964f573`
  (`check_declaration`; `Loaded::run` shortened too). Ticket 279: the six tests again exit 101 on
  `5bf5500`; on `964f573` fmt, clippy, the six tests, test (1030 passed, 0 failed, 7 ignored),
  deny and the release build exit 0; `done in 1298s with exit code 0`.
- This branch sits on T-047 at `f7269b4`. T-047's review round rebased T-047 onto main and
  changed `plugins.rs`, `plugins/declarations.rs` and `cache/mod.rs`; when T-047 merges, this
  branch rebases onto main and runs one more round.
- 2026-11-20T07:57:37+00:00: T-047 merged as `340cf8b`; this branch is rebased onto it (the two `CHANGELOG.md`
  conflicts kept both sides). Ticket 312 ran the six new tests on the rebased tests commit
  `3f1f61b` (each exit 101, as expected) and stopped at clippy: `needless_borrow`, since main's
  memoized `probe_with` returns `&'static [u8]`; fixed in `2320e15`. Ticket 324 on `2320e15`:
  fmt, clippy, the six tests by name, test (1070 passed, 0 failed, 7 ignored), deny and the
  release build exit 0; `done in 577s with exit code 0`. Its logs show `gate`, `gate-host`,
  `gate-runtime`, `gate-sdk` and `gate-plugin-git` compiled from this worktree and the six tests
  listed by name.
- The branch merges after the 0.2.1 tag.
- 2026-11-20T08:31:53+00:00: the lead's review round. `Run::message` replaces `Git::message`, and `Run::git` answers
  only a rule that declares `READS_HISTORY`, so a message-only rule reaches no history and each
  refusal names its flag (probe rules `message-no-flags` and `history-undeclared`). A
  `[hooks] commit-msg` entry that combines `--commit-message` with a tree selector is a
  configuration error. A message run whose selected rules read no message says so on stderr
  (exit 0 stays). `gate/budget` still runs in a message run; design item 2 says so, and item 5
  says `git/commit-message` declares only `READS_MESSAGE`. The `CHANGELOG.md` says
  `git/branch-name` no longer runs in `commit-msg`. `check_request` no longer splits `norms`
  from its doc comment. The selection test asserts that one rule ran. Not yet run in the lane.
- `aa1cf45`: the other-interface load error names no core version as a plugin's release; it says
  to pin the plugin's release built for the interface (each plugin's `CHANGELOG.md` names it) or
  rebuild with gate-sdk.
- 2026-11-20T09:47:41+00:00: ticket 335 on `aa1cf45`, target `T-057/target`: fmt, clippy, the nine review and message
  tests by name, `cargo test --workspace --no-fail-fast` (1074 passed, 0 failed, 7 ignored), deny
  and the release build exit 0; `done in 531s with exit code 0`. Its logs show `gate`,
  `gate-host`, `gate-runtime`, `gate-sdk`, `gate-plugin-git` and the probe compiled from this
  worktree, and the nine tests listed by name.
- 2026-11-20T16:27:51+00:00: rebased onto `main` at `cee6491` (batches 1 and 2). The WIT there is
  already `gate:plugin@0.3.0`, so `reads-message` joins that version; `INTERFACE` and the load-error
  test name it unchanged. Conflicts: `crates/gate/tests/plugins.rs` (both sides add tests; kept
  both), `wit/gate.wit` (the header names both flags), `plugins/git/src/commit_message.rs` (`main`'s
  rule now checks pushed commits: it declares `READS_HISTORY` and `READS_MESSAGE`, reads the message
  through `Run::message` and the commits through `Run::git`). The `CHANGELOG.md` entries move from
  the released 0.2.1 section into 0.3.0's Breaking Changes, without a second interface-bump line.
  Not yet run in the lane.
- Ticket 347 on `1b8f4bc`: fmt, clippy, the eleven tests by name and the release build exit 0;
  `cargo test --workspace` 1350 passed, 1 failed:
  `the_rules_read_history_or_the_message_over_the_repository` still expected `git/commit-message` to
  declare only the message; fixed in the next commit. `cargo deny` failed only on RUSTSEC-2026-0587
  (`main`'s lock before T-073). Ticket 355 was stopped in the queue so T-073 could land first.
- 2026-11-20T17:59:17+00:00: rebased onto `main` at `7f62554b` (T-073, T-074) without conflict; the
  scope-less header of the CHANGELOG commit is now `docs(changelog): ...`, as the repository's own
  `commit-msg` hook requires. Not yet run in the lane on this tip.
- 2026-11-20T20:10:55+00:00: ticket 368 on `f6720d4b`, target `T-057/target`: fmt, clippy, the twelve tests by
  name, `cargo test --workspace --no-fail-fast` (1351 passed, 0 failed, 7 ignored), `cargo deny`
  (advisories, bans, licenses, sources ok) and the release build exit 0; `done in 1140s with exit
  code 0`. Its logs show `gate`, `gate-host`, `gate-runtime`, `gate-sdk`, `gate-plugin-git` and
  `gate-runtime-probe` compiled from this worktree, none from another, and the twelve tests listed
  by name.
- 2026-11-20T20:19:36+00:00: the review check of the fix round: every finding fixed in code, one blocking documents
  fix. Rebased onto `main` at `18c0bf82` (T-075, T-067, T-078), then onto `4d1f4fe4` (T-079,
  documents only, no conflict); conflicts only in `CHANGELOG.md`
  and `crates/gate/tests/plugins.rs`, both sides kept; T-075's `commit_message.rs` merged without
  conflict. The git plugin's `### Changed` block moves out of the released 0.2.1 section into its
  0.3.0 Breaking Changes, without the sentence that said `git/commit-message` no longer reads
  history (false since `5089a2b1`); the 0.2.1 section is identical to `main`'s. The root
  `CHANGELOG.md` names `--base` among the refused selectors, as the code does; the guide's long
  lines are rewrapped. The warning about a message check that ran no rule was already never
  printed with `--list-changed-projects`, which returns before it; the warning test now proves
  it. Not yet run in the lane on this tip.
- 2026-11-20T20:50:01+00:00: ticket 377 on `0e2abf1a`, target `T-057/target`: fmt, clippy, the twelve tests by
  name, `cargo test --workspace --no-fail-fast` (1446 passed, 0 failed, 7 ignored), `cargo deny`
  (advisories, bans, licenses, sources ok) and the release build exit 0; `done in 761s with exit
  code 0`. Its logs show `gate`, `gate-host`, `gate-runtime`, `gate-sdk`, `gate-plugin-git` and
  `gate-runtime-probe` compiled from this worktree, none from another, and the twelve tests listed
  by name.
- Next: the lead merges; then the consumer's cold `commit-msg` row, before and after.

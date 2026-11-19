# T-057: a `--commit-message` check checks only the message

| Field | Value |
| --- | --- |
| Status | task file written; design item 1 (how a rule declares the message) waits for the lead |
| Release | v0.3.0 (it changes what an existing `commit-msg` hook checks) |
| Writer | "Gate 006: nestjs semantic rules" |
| Reviewer | a separate review session |
| Depends on | T-047 (a check loads only the plugins it needs) |
| Touches | `crates/gate-sdk` (the rule declaration, `Git::message`), `wit/gate.wit` under (b), `crates/gate-host/src/{lib.rs,plugins.rs}`, `crates/gate/src/main.rs`, `plugins/git` (the rules that read the message); `docs/guide/getting-started.md`, `docs/guide/configuration.md`, `CHANGELOG.md` (Changed) |

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

1. **A rule declares that it reads the message.** The lead's direction is that the rules whose
   `NEEDS` include the message are the ones that run. Today `Rule::NEEDS` is a list of `Need
   { key, when, why }`, where `key` is a key of `[plugins.<name>] options`. The list travels in
   the options schema under `x-gate-needs`, and `check_needs` refuses the configuration when an
   enabled rule's key is missing. So the message cannot be one more `Need` as the type stands:
   `check_needs` would look for an option called `message` and fail the configuration. Two ways
   to carry it:
   - (a) In `NEEDS`, as the lead directs. `Need` gains a kind, either an option key (as today)
     or the message, and the SDK has one constant for the second, `Need::MESSAGE`.
     `check_needs` skips that kind, and the run selection reads it from `x-gate-needs`. The
     interface stays `gate:plugin@0.2.0`, since the schema is a string.
   - (b) A constant beside `READS_HISTORY`, `const READS_MESSAGE: bool`, carried in `rule-info`
     as `reads-message`. That makes the interface `gate:plugin@0.3.0`.

   Either way, `Git::message` answers only to a rule that declares it, as `Run::git` answers only
   to a rule that declares `READS_HISTORY`. A rule that reads the message without the declaration
   gets an error that names it, so the selection below cannot miss a rule that reads the message.
   A project-scope rule that declares it is refused when its plugin describes itself, because the
   message belongs to the repository. Writer's view: (b) keeps `NEEDS` to one meaning, options
   the configuration must hold. (a) keeps the interface version. The lead decides; the rest of
   this file holds for both.
2. **What the run selects.** A check with `--commit-message` runs only the enabled rules that
   declare the message, in the repository scope. `--rule` narrows that further, as anywhere
   else. The core's own rules do not run. The summary says `commit message`.
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
5. **The `git` rules.** `git/commit-message` and `git/no-ci-skip` declare the message. Both
   also keep `READS_HISTORY`: `no-ci-skip` checks pushed commits in `pre-push`. A rule that reads
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
- A rule that calls `Git::message` without the declaration gets the error that names it.
- A project-scope rule that declares the message fails its plugin's load as broken.
- Under (a), an enabled rule that needs the message passes `check_needs` with no such option.
- `check --commit-message <file> --staged` is a usage error that names `pre-commit`.
- The summary of a message run is `gate check (commit message): ...`.

## Done when

- The tests above, each failing before its change.
- The consumer's cold `commit-msg` row, before and after, with the command and the lane ticket.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

Branched from main `73d33e7`. Task file written; nothing implemented. Next: the lead's decision
on design item 1, (a) or (b), and on items 3 and 4; then the tests, each failing first.

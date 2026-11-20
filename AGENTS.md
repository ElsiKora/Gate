# Gate: rules for every agent

Read this file when your session starts, then your task file under [docs/tasks](docs/tasks), then the
local rules file your brief names (machine and consumer boundaries kept outside the repository). These
rules bind every writer and every reviewer.

## What this repository is

Gate is a modular repository linter and gate written in Rust. The core is the tool: configuration,
discovery of projects in a repository, change selection from git, the plugin runtime, findings,
reporting, caching, hook and CI generation, `doctor`, installation. The core holds no rule. Every rule,
every generator of files and every language is a plugin, pinned by version and checksum. The design is
[docs/architecture.md](docs/architecture.md); the state of the work is [docs/plan.md](docs/plan.md).

## Clean, no legacy

- There is no earlier version to stay compatible with. No alias of an old option, command, rule id or
  file format; no reader of an old configuration; no "kept for compatibility" branch.
- One way to do each thing: one way to install the tool, one way to install a plugin, one plugin form.
- The core never names a language, a framework, a product or a file layout. A rule never reads the tree
  except through the plugin interface.

## Public from the first release

This repository becomes public. Everything committed must be fit for that:

- No name of a private project, product, game, operator or person; no machine path, account id, IP or
  e-mail address; no secret. Fixtures and test trees are synthetic, written for the test.
- English everywhere: code, documents, commit messages, tags, releases.
- No AI attribution anywhere (no `Co-Authored-By` for an agent, no "Generated with").

## Work

- One task = one task file under [docs/tasks](docs/tasks) = one branch = one worktree = one writer.
  The task file says what done is; keep its `Resume point` current.
- Conventional Commits: `feat(host): ...`, `feat(plugin-git): ...`, `fix(sdk): ...`, `test(...)`,
  `docs(...)`, `ci(...)`, `chore(...)`, `refactor(...)`, `perf(...)`, `style(...)`, `revert(...)`. The
  message says what changed, why, and what was measured.
- Gate checks this repository with its own released core and plugins ([gate.toml](gate.toml)). Run
  `bin/gate sync` once in a clone to turn on the hooks; CI runs the same checks on every push.
- Commit and push every 10 to 15 minutes and at every green checkpoint.
- Code is reviewed before it merges by a session that did not write it. One fix round, then the lead
  decides.

## Proof

- "Done" needs a command and its exit code, run in the session that says so. Every number in a document
  or a message comes from a command that was run; name the command. Timestamps come from `date -Iseconds`.
- Tests cover what would let a broken change through a gate or block a good one: each rule's pass and
  fail, project selection, change selection, configuration errors, checksum verification failing closed,
  cache invalidation. No tests for wiring.
- A ported rule is accepted when its plugin reports the same findings as the reference
  implementation on the same trees. The comparison runs outside this repository; the task file
  names it.

## Code

- Rust stable as pinned in `rust-toolchain.toml`; `unsafe` is forbidden; `cargo fmt`, `clippy -D warnings`
  and `cargo deny` are clean.
- Errors say what is wrong and what to do. No `unwrap` outside tests.
- Public items of the SDK and the plugin interface are documented; every rule has a page.

## Commands

```bash
cargo build --release --locked
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo deny --locked check
```

## What

<!-- What this pull request changes, in one or two sentences. Link the issue: "Closes #<number>". -->

## Why

<!-- The problem it solves, or the decision it follows. -->

## How it was checked

<!-- The commands you ran and their exit codes. Every number in this description comes from a command. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `cargo test --locked`
- [ ] `cargo deny --locked check`

## Checklist

- [ ] The title is a Conventional Commit (`feat(host): ...`, `fix(plugin-git): ...`).
- [ ] New or changed behaviour has a test: a rule's pass and fail fixtures, or the case that failed before.
- [ ] The guides and rule pages it affects are updated.
- [ ] No rule, language or framework name in the core; no compatibility alias.

# T-029: `git`: branch names and commit messages as ElsiKora's own linters check them

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 002: plugin form spike" |
| Reviewer | a separate review session |
| Depends on | `release/v0.1.0` (from its tip after the v0.1.0 short round) |
| Touches | `plugins/git/**` (the branch-name rule, one new rule, their pages and schemas), the plugin interface and host only for item 2's message, their tests, architecture §7's example |

## Goal

A repository that uses ElsiKora's public linters today gets the same verdicts from the `git` plugin,
with no Node step:

1. `git/branch-name` accepts what `@elsikora/git-branch-lint` accepts and refuses what it refuses
   (reference: `ElsiKora/git-branch-lint` at `9718c1a`, `src/domain/policy/branch-template.policy.ts`
   and `src/application/use-cases/lint-branch-name.use-case.ts`).
2. A new rule, `git/commit-message`, checks the message of the commit under review the way
   `@commitlint/config-conventional` with ElsiKora's rule set does (reference: the `rules` of
   `commitlint.config.js` in the same repository).

## Scope

1. `git/branch-name` options, alongside `pattern`, `min-length` and `max-length`:
   - `template`, a placeholder template such as `":type/:ticket-:name"`. `:type` is one of `types`;
     `:ticket` is `[a-z]{2,}-[0-9]+`; any other placeholder is `subject`; a placeholder followed by
     `-` is optional (the name is valid with it and without it); runs of `/`, `-`, `_`, `.` collapse
     and delimiters at the ends are trimmed, as the reference does. Exactly one of `template` and
     `pattern` is given.
   - `types`, the branch types (`feature`, `bugfix`, `hotfix`, `release`, `support` in the reference
     default).
   - `subject`, the regex of a free placeholder (default `[a-z0-9-]+`), or a table per placeholder.
   - `prohibited`, names refused outright (the reference default: `main`, `master`, `release`). The
     reference lints the branch a developer works on, in hooks; so does this option: it applies to
     the branch a push updates and to the checked-out branch in hook runs (`--staged`,
     `--commit-message`, `--push`), and not to a plain or CI `gate check`, whose checkout of main
     must pass. The page says this.
   - `ignore`, names that skip every check but `prohibited`.
   - The order of checks is the reference's: prohibited, ignore, min length, max length, pattern.
     Each finding names the expected template and the valid types, or the prohibited list, as the
     reference's hints do.
2. `git/commit-message`: the subject (header), scope, body and footer checks of the reference rule
   set: type in an enum (default the reference's twelve), scope required and lower case at most 30
   characters, subject lower case without a final full stop, 3 to 80 characters, header at most 100
   and at least 10 (a warning, as in the reference), body and footer after a blank line, lines at
   most 100, body ending in a full stop. Every limit and the enum are options with the reference's
   values as defaults. A level the reference sets to 1 is a separate aspect whose default level is
   `warning`.
   It reads the message `gate check --commit-message <file>` passes. Today the host reads that file
   and hands it to no plugin. Add the message to what a rule can read through the plugin interface,
   additively, documented in the SDK; a run without a message makes the rule check nothing. Merge,
   fixup, squash and revert messages that git writes are handled as the reference handles them
   (commitlint ignores them by default).
3. Rule pages in the house style, with the options table, an example finding and how to fix it.
   Architecture §7's example names only rules that exist.
4. Tests (synthetic names and messages only), each failing before the change:
   - branch names: every placeholder kind, the optional ticket present and absent, delimiters
     collapsed, each prohibited name in a hook run and the same name passing in a plain run, ignore,
     both length limits, a template and a pattern together refused at load;
   - commit messages: one pass and one fail per check, the warning aspects, a message from a file
     with a comment section (`#` lines are not part of it), and a run without a message.

## Done when

- The tests above.
- A comparison outside the repository: a table of at least 60 branch names and 60 messages run
  through the reference tools (git-branch-lint 9718c1a; commitlint with that config) and through
  `gate check`; every verdict the same, every difference explained. The lead names the table's
  location in the plan.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `cargo test --locked --workspace`, `cargo deny --locked check` and `cargo build --release --locked`
  all exit 0 in the gates on the pushed commit.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-18: planned; branch `task/T-029-git-branch-and-commit-format`.
2026-11-18: branch from `release/v0.1.0` `d30ecb8` (rebase on its tip after the v0.1.0 short round).
The references are read: git-branch-lint `9718c1a` (template policy, lint use case, default
configuration) and commitlint 20.5.3 with config-conventional 20.5.3, the conventionalcommits preset
9.3.1 and conventional-commits-parser 6.4.0, installed outside the repository for the comparison.
Decided (lead): the generated hook scripts pass `--hook`, the plugin interface answers `hook()`,
and `prohibited` applies only when a hook runs, so a CI `--push` to main passes.
2026-11-18: done on the branch: the interface (`message()`, `hook()`, the hooks' `--hook`),
`git/branch-name` templates, `git/commit-message` (one aspect per commitlint rule), both pages,
architecture §4 and §7, the plugin changelog. Native `cargo test -p gate-plugin-git` and its clippy
pass. Open: how the two warning aspects get `warning` by default (asked the lead; the page shows
`levels` meanwhile). Next: the 60 + 60 comparison, then a lane round.
2026-11-19: decided (lead): a preset sets aspect levels (`levels`), after the project's aspect level
and before its rule level; `git/recommended` gives the two warning aspects `warning`. Lengths count
UTF-16 code units, as the references do. The native comparison (147 inputs: 69 branch names, 78
messages) agrees on every verdict. Added (lead): a rule declares the plugin options it reads
(`Rule::NEEDS`, `x-gate-needs` in its options schema) and the host refuses an enabled rule's missing
key at load (exit 2), naming the rule. Next: a lane round (the host and CLI tests compile only
there), then the comparison through the built `gate check`.
2026-11-19: rebased on the public main `59fb163` (the plugin changelog's entries moved under
Unreleased, above its 0.1.0). The first lane round on the old base failed clippy only
(`too_many_lines` in the missing-option test, now two tests); it was withdrawn for the move. Next: the
lane round on the rebased tip, then the comparison through the built `gate check`.

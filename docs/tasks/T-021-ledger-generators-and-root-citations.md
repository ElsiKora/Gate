# T-021: Ledger count generators and repository-root citations

| Field | Value |
| --- | --- |
| Status | in review |
| Release | v0.1.0 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a session that did not write it |
| Depends on | T-018 (merged) |
| Touches | `plugins/ledger`, `plugins/agents` (instruction-references), their rule and generator pages |

## Goal

1. The ledger plugin has the two generators a consumer's `[generate]` names: `ledger/decision-counts`
   (the counts block of the decision index) and `ledger/review-progress` (the progress table of a
   roadmap). Their output is the reference implementation's, byte for byte; the problems the
   reference reported while it generated (a malformed `Counts:` line, a feature without a review
   status, a queue row without a record) are findings of `gate/sync#<generator>` with their line.
2. `agents/instruction-references` in a project of a workspace accepts a backticked path that names a
   file, a directory or a matching glob from the repository root, when the configuration grants the
   plugin that path (`[plugins.agents] reads`). A path that resolves neither in the project nor from
   the root is reported as before.

## Scope

1. `ledger/decision-counts`: in the block `<!-- gate:begin decision-counts -->` of the decision index,
   `- Decision IDs: `<n>`` (every index row, repeats included) and `- Superseded decisions: `<n>``
   (decisions whose status starts with `superseded`). Nothing is generated while the index is not
   tracked. Option `decisions`.
2. `ledger/review-progress`: in the block `<!-- gate:begin review-progress -->` of the roadmap, the
   table of review counts from the journal's `Counts:` lines, the inventory's `- Review:` statuses, the
   execution queue's records and their implementation statuses, and the decision index. Options `path`,
   `decisions`, `implementation`, `cross-cutting-kinds`, `zero-answer-kinds`, the journal and queue
   headings and the review vocabulary.
3. Both write only the file they name, so they own no glob: a missing block is a finding, and the file
   is never deleted.
4. Generators receive their own options only (the interface passes no `[plugins.ledger] options` to
   `generate`), so `decisions` and `implementation` are generator options whose defaults are those of
   `[plugins.ledger] options`.
5. `agents/instruction-references`: in a project at `<root>`, a backticked path that is not found in
   the project is looked up as a repository path among the granted paths the project's tree lists
   (`../…`). Its first segment may be a top-level name of the project or of a granted repository path.
6. Pages: the two generators on the ledger page, the fallback on the rule's page.

## Tests

- decision-counts: the block written with the counts; a repeated row counted; superseded counted;
  no output for an untracked index; a missing or repeated marker is a finding with its line.
- review-progress: the table byte for byte on a synthetic roadmap; each problem with its line;
  kinds from the options in the row labels and the `Counts:` grammar; option errors.
- instruction-references, a project in a workspace: a root file, a root directory and a root glob
  with a match pass when granted; a granted root's missing file, directory and glob are reported;
  without a grant a root path is judged as before (prose when its first segment is no top-level name
  of the project, reported when it starts with `.`); a path inside the project behaves as before.

## Done when

- fmt, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked
  --workspace`, `cargo deny --locked check` and `cargo build --release --locked` exit 0 on the
  pushed commit.
- Do not edit docs/plan.md.

## Resume point

2026-11-18: planned; from main 86b7b04 on `task/T-021-ledger-generators-root-citations`.
2026-11-18: both generators in `plugins/ledger/src/generators`, ported from the reference's roadmap
and decision-index counting; the kinds, headings and review vocabulary are options. They own no
glob (each writes only the file it names). `decisions` and `implementation` are generator options
with the `[plugins.ledger] options` defaults, since `generate` receives no plugin options.
instruction-references resolves a path not found in the project among the granted paths outside it.
Plugin tests: ledger 80 passed, agents 24 passed; clippy and fmt clean for both plugins.
2026-11-18: review fix round. A feature is exactly `F` and three digits; a configured kind of that
form, or one listed twice, is refused. An untracked decision index is a finding (counts 0). Queue
problems carry the row's line, with one message for a missing record and one for a record without a
status. The `Counts:` message says "must directly follow". instruction-references applies the
project's `.gitignore` only to a path read inside the project, and the root `.gitignore` (when
granted) to a path read from the root. The ledger block keeps a CRLF file CRLF. Plugin tests: ledger
82 passed, agents 25 passed; clippy and fmt clean.

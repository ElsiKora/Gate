# T-059: The decision a rule entry names is an accepted decision of the index

| Field | Value |
| --- | --- |
| Status | in progress (design approved by the lead, 2026-11-19) |
| Release | v0.3.0, in the same interface bump as T-057 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | T-048 (`decision` on rule entries): the branch sits on its tip `52b2935`, as T-052 does. T-057 makes the same `gate:plugin@0.3.0` bump; whichever of the two merges second rebases the WIT and SDK lines |
| Touches | `wit/gate.wit`, `crates/gate-runtime/src/runtime.rs` (the interface version, the request's paths), `crates/gate-host/src/plugins.rs` (the request), `crates/gate-host/src/config` (the decisions of a file), `crates/gate-sdk` (`RuleDecision`, `run.rule_decisions()`), `plugins/ledger` (one new rule and its page), the request literals of the plugins' tests, the runtime's probe, tests, docs, `CHANGELOG.md`, `docs/plan.md` (one row) |

## Goal

T-048 lets a rule entry, or a `"<plugin>/*"` family, name the decision that accepted its relaxation,
and lets `[decision-policy] require-decision` make that mandatory. The host checks that a decision
is named; nothing checks that the decision exists and is accepted. For exceptions,
`ledger/exception-decisions` does: it reads `gate.exceptions.toml` through the tree and looks each
`decision` up in the decision index. A rule entry's decision deserves the same check, so that a
relaxation cannot cite a decision that was never taken, was rejected, or is still `proposed`.

## 1. What is checked, and by whom

Checked: each `decision` written on a rule entry or a family entry, in the `gate.toml` that belongs
to the scope of the call, names a decision the index (`[plugins.ledger] options` `decisions`) lists
with the status `accepted`. Two findings, as for exceptions: the index does not list it
(`#unlisted`), or lists it with another status (`#not-accepted`). The finding points at the entry's
line in that `gate.toml`. A rule entry without `decision` is not this rule's finding; whether one is
required stays the host's load-time check from T-048, as it is for exceptions.

The checker: a new rule of the ledger plugin, `ledger/rule-decisions`, project scope, beside
`ledger/exception-decisions`, sharing its index loading and its messages. A new rule, not a wider
`ledger/exception-decisions`: that name says what it covers, and a consumer may want one without the
other.

The host cannot do it alone. The decision index, its file format and what `accepted` means are the
ledger plugin's: the core never names a layout or a product, and holds no rule. The host knows each
written decision with its file and line (T-048 keeps `decision` beside the level, and
`SettingOrigin` holds the file and line), but it cannot judge one.

The plugin can do it alone, with no interface change, only by reading `gate.toml` itself through the
tree, as it reads `gate.exceptions.toml` today. Not recommended:

- It is a second reader of the core's whole configuration format: the string and table forms of a
  rule entry, family keys, `level` and `levels`, `[[profiles]]`, and every later change to them. When
  the two readers drift, a decision the plugin cannot parse is a decision it does not check, and a
  broken change passes. `gate.exceptions.toml` is a small file with a published schema
  (`gate schema --exceptions`); `gate.toml` is the core's configuration.
- The WIT header says levels, norms and exceptions belong to the core's configuration. The interface
  already hands configuration that rules need across, read-only: `check-request.norms` is the
  precedent, in the same record.

## 2. The interface addition

One record and one field, nothing else:

```wit
/// A decision a rule entry or a family entry of the scope's `gate.toml` names: the key as written
/// (`<plugin>/<rule>`, or `<plugin>/*` for a family), the decision's id, and where it is written.
record rule-decision { key: string, decision: string, at: file-location }

record check-request {
  // ... as today, then:
  /// Every decision the rule entries of the scope's own `gate.toml` name, for rules about them.
  rule-decisions: list<rule-decision>,
}
```

`at` reuses `file-location`: a path relative to the scope's root, as every path a plugin addresses,
and a span holding the entry's line. The SDK gets `run.rule_decisions()` beside `run.norms()`, on
both run types, documented. `rule-info` does not change: the list is small and goes to every call,
as `norms` does.

What the host puts in the list:

- The decisions written in the scope's own `gate.toml`: a project's file in its project's call; the
  root file in the root project's call when the top is a project, the same placement as the root's
  exceptions (`workspace.rs`). A project's call does not carry the root file's decisions, because a
  path relative to the project's root cannot name a file above it.
- Every written decision, from the file as written, whether or not a later layer overrides the entry
  and whether or not a policy requires it: a decision that is written is checked.
- In `[rules]` and in the `rules` of `[[profiles]]`; the key as written, so a finding names the
  entry the reader sees.

Out of scope (lead, 2026-11-19), named as a follow-up the lead records: **the root file without a
root project**. In a repository whose top is not a project, the root file's decisions, like the
root's exceptions (`gate.exceptions.toml` beside the root `gate.toml`), reach no ledger rule. One
follow-up covers both: a repository-scope way to check them against the index read from the
repository root, for exceptions and rule decisions alike.

Not taken: delivering exceptions through the same list so that `ledger/exception-decisions` no
longer reads `gate.exceptions.toml`. `ledger/exception-owners` still needs the file for `owner`, so
the plugin keeps one reader of it either way; it would widen this change for no fewer readers.

## 3. Tests

What would let a broken change through, or block a good one (as written: `crates/gate/tests/plugins.rs`
through the runtime's probe rule `probe/decisions`, which reports each decision of its call, and
`plugins/ledger/tests/rules/rule_decisions.rs`):

- Host, the request: a decision on a rule entry and on a family entry in a project's `gate.toml`
  arrives in that project's call, with the key as written, the id trimmed, the path `gate.toml`
  relative to the project's root and the entry's line. Another project's call does not carry it.
- Host, placement: under a root project, the root file's decisions arrive in the root project's call
  and in no other; without a root project, they arrive in no call.
- Host, content: an entry without `decision` adds nothing; a decision on an entry a project file
  overrides still arrives in the call of the file that wrote it; the list is the same with
  `require-decision` true, false or absent.
- Plugin, pass: an accepted decision gives no finding, for a rule entry and for a family entry.
- Plugin, fail: a decision the index does not list gives `#unlisted` at `gate.toml:<line>`; a
  `proposed` one gives `#not-accepted` naming the status; an index that is absent or not granted
  reads as listing nothing, as for exceptions; the finding names the key as written.
- End to end, with the ledger plugin built: a fixture with `"demo/x" = { level = "off", decision =
  "D-A-001" }` exits 1 with the finding while the index says `proposed`, and 0 once it says
  `accepted`.
- The rule's page, with its example finding run against a synthetic tree.

## Built

- `wit/gate.wit`: `record rule-decision` and `check-request.rule-decisions`, and the package at
  `gate:plugin@0.3.0` (the line T-057 writes too); `docs/architecture.md` shows both.
- Host: `Config::rule_decisions` lists every decision of a file's `[rules]` and `[[profiles]] rules`
  with the line of the entry's value; `plugins.rs` fills a project's call from
  `Workspace::project_config` (the root file for the root project) with repository paths and
  `Span::line`, and the repository call with none. The runtime gives the plugin each path relative
  to its grant's root, as it does an options file. The result cache key holds the request, so a
  changed decision is not answered from the cache.
- SDK: `RuleDecision`, `CheckRequest::rule_decisions`, `Call::rule_decisions` and
  `Run::rule_decisions`; the writing-a-rule guide says a rule never reads `gate.toml` and names
  both accessors.
- Ledger: `ledger/rule-decisions` (no options) and its page; `index::refusal` is the one judgement
  of a named decision, which `ledger/exception-decisions` now uses too (its messages unchanged).

## 4. Recommendation

v0.3.0. The release already bumps the interface to `gate:plugin@0.3.0` for T-057, and every plugin is
rebuilt against it; this addition is one record and one field. Waiting means a 0.4.0 interface bump
for this alone, and until then a required decision can name anything. Order: T-048 first (the field
exists), then this task, with T-057 in either order (both touch `wit/gate.wit` and the SDK's run
types; the second rebases). Size: the host fill, like `norms`; one ledger rule, with the exception
rule's index loading and messages shared; one page; one SDK accessor.

If v0.3.0 is to stay as it is, the interface addition waits for 0.4.0; a plugin that parses
`gate.toml` in the meantime is not a step towards it, and is not recommended.

## Done when

- One lane round on the pushed tip: fmt, clippy, test (`--workspace --no-fail-fast`), deny and the
  release build exit 0, and the leak scan prints nothing.
- docs/plan.md: only the T-059 row (lead, 2026-11-19).

## Proof

Round 317 (ticket 000000000317) on `21d2d53`, rebased onto main at `e9e1434`, with a target of
its own (`CARGO_TARGET_DIR` per task), from 2026-11-20T06:14:19+00:00 to 2026-11-20T06:36:17+00:00:
`cargo fmt --all --check` exit 0, `cargo clippy --locked --workspace --all-targets -- -D warnings`
exit 0, `cargo test --locked --workspace --no-fail-fast` exit 0, `cargo deny --locked check` exit 0,
`cargo build --release --locked` exit 0, leak scan exit 0. The log has `Compiling` from this
worktree for every crate the branch changes, and each of the 28 tests the branch adds over T-048's
tip `52b2935` passes by name. Earlier green rounds 261 and 308 shared a cargo target with other
worktrees and are not counted.

## Resume point

2026-11-20: code on T-048's work, rebased onto main at `e9e1434`; round 317 green (see Proof).
Next: review.

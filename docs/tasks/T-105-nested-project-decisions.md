# T-105: decisions named in a nested project are checked against the workspace's index

| Field | Value |
| --- | --- |
| Status | done (reviewed, one fix round and one correction; gates green on `0f8e24db`; merged) |
| Release | the next `ledger` plugin release, and the core release if the core must hand more to the rule; the plugin interface stays `gate:plugin@0.3.0` |
| Writer | "Gate T-105" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `80db512` |
| Touches | `plugins/ledger/` (the decision rules, their pages, CHANGELOG), and the core only where it hands decisions to a rule; tests; `docs/plan.md`, this task file |

## Goal

A workspace has a root project and a nested project with its own `gate.toml`. The decision index is
at the repository root. Its options are in the root's `[plugins.ledger]`.

Today:

- `ledger/rule-decisions` (project scope) checks the decisions written in the project's own
  `gate.toml` against `[plugins.ledger] options decisions`.
- In the nested project the index path resolves relative to the nested project's folder, so it
  finds no index. Every decision named there, valid or not, is reported as "does not list". A path
  such as `../../docs/...` is accepted but reads nothing.
- `ledger/root-rule-decisions` reports nothing when the root is a project.

So a decision named in a nested project's `gate.toml` is checked by no rule. The first consumer
names 15 there and holds them by review until Gate checks them.

## Required outcome

1. Every decision named in any `gate.toml` of the workspace is checked against the workspace's
   decision index: in `[rules]` entries, in families, in `[[profiles]]` rules, and in exceptions
   that name a decision.
2. Each written decision is judged exactly once, by one rule.
3. A decision missing from the index, or not `accepted`, gives one finding at the entry's line in
   the file that wrote it.
4. The plugin interface stays `gate:plugin@0.3.0`. If the outcome cannot be reached without
   changing it, stop and report before writing code.
5. No alias of an old rule id and no second way to configure the index. If a rule is merged or
   renamed, its old id is gone and its page says so in the CHANGELOG only.

## First step: the design, to the lead

Before code, send the lead a short design that answers these:

- which rule judges a nested project's entries;
- where it reads the index;
- what the core already hands a repository-scope rule;
- what changes for a workspace whose root is not a project;
- how the exception rules (`ledger/exception-decisions`, `ledger/exception-owners` and their root
  forms) behave for a nested project's exceptions.

The lead answers within the session.

## Proof

- Synthetic fixtures with a root project and a nested project, both writing decisions:
  - valid ones pass;
  - a missing one and a `proposed` one each give exactly one finding, at the right file and line;
  - a root that is not a project still works;
  - no entry is reported twice.
- The rules' pages, examples and CHANGELOG match.
- All cargo runs go through the lane.

## Design (approved by the lead, 2026-11-26)

- One repository-scope rule, `ledger/config-decisions`, judges every decision written in the
  workspace's configuration. It replaces `ledger/rule-decisions`, `ledger/root-rule-decisions`,
  `ledger/exception-decisions` and `ledger/root-exception-decisions`; the old ids are gone, and the
  ledger presets name the new rule at the level the old ones had.
- The core's repository call carries, in `check-request.rule-decisions`, every decision written in
  the workspace, whether or not the root is a project: the rule entries, families and
  `[[profiles]]` rules of every `gate.toml`, and the `decision` of every exception of every
  `gate.exceptions.toml` (key: the exception's `rule`; at: its `[[exception]]` line). A project's
  call carries none. `config-file` is unchanged.
- The index is the root's `[plugins.ledger] options decisions`, relative to the repository root.
- The plugin interface stays `gate:plugin@0.3.0`: the WIT types are unchanged; the doc comments of
  `rule-decision`, `check-request.rule-decisions` and `check-request.config-file` state the new
  contract. Ledger 0.3.2 and core 0.3.3 need each other; both CHANGELOGs say what each mixed pair
  does.
- The rule is repository-scope, so the T-107 filter (a project call's findings on files a nested
  project owns are dropped) does not touch it.

## Left out

- `ledger/exception-owners` and `ledger/root-exception-owners` read the implementation records
  relative to their scope root, so in a nested project that inherits the root's ledger options they
  read a path inside the nested folder; that defect of the owners rules is T-109.

## Proof run

Lane round on `0b0c88a3` (2026-11-26T08:30:17+00:00 to 2026-11-26T08:48:48+00:00), in a fresh
detached worktree:

| Command | Exit |
| --- | --- |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 |
| `cargo test --locked` (1654 passed, 0 failed) | 0 |
| `cargo deny --locked check` | 0 |

Before it, `cargo clippy --locked --workspace --all-targets --keep-going -- -D warnings` alone on
`0b0c88a3`: exit 0. Earlier rounds failed on clippy only (a too-long `Plugins::run`, a
`useless_format`, a `single_element_loop`) and on two wrong test expectations, each fixed in its
own commit.

## Review fix round

The review (one round) asked for:

- S1: a repository-scope rule that a project's scope turns on (a preset, a family or a `[rules]`
  entry) while the repository scope does not is a configuration error (exit 2), in the core,
  naming the rule, the project and the entry. The plugins guide shows the root half of the ledger
  example; both CHANGELOGs carry the change.
- S2: one compatibility sentence in both CHANGELOGs.
- N1: end-to-end tests for a family with a decision padded with spaces, a second exception that
  fails, and the S1 error.
- N2: the ledger's crate doc wraps at 100.
- N3: an exception's finding reads "the exception for <rule>", as in 0.3.1.

`origin/main` (T-104 and T-106) was merged first; the planning loop of `Plugins::run` walks
`Loaded::enabled` and keeps this task's `check_request`. Of the configurations on the machine in
Gate's format, none turns on a repository-scope rule from a project alone, so S1 breaks none of
them. Two test fixtures did: the decision-policy tests' family in a profile (the root now turns on
the probe's repository-scope rules too) and `probe/both` extended in a profile alone (now the S1
error, with a case that extends it in both places).

Fix round proof, on `056ed98f` (with `origin/main` at `025cd579` merged), in fresh detached
worktrees through the lane:

| Command | Exit |
| --- | --- |
| `cargo clippy --locked --workspace --all-targets --keep-going -- -D warnings`, alone (2026-11-26T09:44:09+00:00 to 09:46:11+00:00) | 0 |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 |
| `cargo test --locked` (1667 passed, 0 failed) | 0 |
| `cargo deny --locked check` | 0 |

The full round ran from 2026-11-26T09:46:17+00:00 to 2026-11-26T10:07:56+00:00. Before it, the
gate, gate-host and ledger tests alone on `74e4cd9c` found one wrong expectation of the new family
fixture (636 passed, 1 failed), fixed in `056ed98f`.

## Correction after the fix round

The second review found one blocker (B1): a root that sets a repository-scope rule `off` on purpose
(for example `ledger/root-exception-owners`, with its decision) was refused when a project's preset
or family turned the rule on, and a profile cannot set it. The check now asks whether the root's
configuration names the rule: a root `[rules]` entry, a root family or a root preset, at any level,
`off` included. Only a rule the root never names is refused, so a ledger declared only in a project
still is. The message, the guides and both CHANGELOGs say "turn the rule on or set its level
there"; a test loads a root `off` with its decision under the policy beside a project preset and,
separately, a project family.

Proof of the correction, through the lane in fresh detached worktrees: clippy alone
(`--workspace --all-targets --keep-going`) on `ab5abdce`, 2026-11-26T10:16:51+00:00 to
10:20:25+00:00, exit 0; the full round on `0f8e24db` (`ab5abdce` with `origin/main` at `f026fba1`
merged, task files only), 2026-11-26T10:20:31+00:00 to 10:41:58+00:00:

| Command | Exit |
| --- | --- |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 0 |
| `cargo test --locked` (1667 passed, 0 failed) | 0 |
| `cargo deny --locked check` | 0 |

## Resume point

Done on the writer's side: the B1 correction is green on `0f8e24db` and pushed. Waiting for the
lead.

# T-072: Exceptions and rule decisions at a workspace root that is not a project

| Field | Value |
| --- | --- |
| Status | Option 1 approved; lane ticket 411 green on `8ed4f919` (on `main` `6b36eeac`); for review |
| Release | v0.3.0 (the request's meaning for the repository call, on `gate:plugin@0.3.0`, unreleased) |
| Writer | "Gate 006" |
| Reviewer | a separate review session |
| Depends on | the stack tip `origin/task/T-052-check-time-thresholds` at `0ac71f9` (T-048 decisions and the decision policy, T-059 `rule-decisions` and `ledger/rule-decisions`), on `main` at `d0c6cda` (T-071, `[workspace] extends`) |
| Touches | `wit/gate.wit` (`check-request`), `crates/gate-host/src/plugins.rs`, `crates/gate-sdk` (the request), `plugins/ledger` (three repository-scope rules, their pages, the preset), the runtime's probe component, `docs/guide/configuration.md`, `docs/architecture.md` |

## Goal

In a monorepo whose root is not a project, the root `gate.toml` turns on repository-scope rules
(`[rules]`, and since T-071 `[workspace] extends`), and the root `gate.exceptions.toml` holds their
exceptions. Both may name a decision, and the root's `[decision-policy] require-decision` can
require one, but no rule checks that decision against the decision index. A root can therefore relax
a rule or suppress a finding by naming a decision that is unlisted or only proposed, and every check
passes. A project's file is held to its decisions; the root file should be held to the same.

## Current behaviour, on the stack tip `0ac71f9`

Read from the code; each point names where it is decided.

1. **The root file's decisions reach no call.** `crates/gate-host/src/plugins.rs:952` builds each
   call's `rule-decisions` with `project.map_or_else(Vec::new, |name| rule_decisions(workspace,
   name))`. The repository call (`project` is `None`) always gets an empty list, and
   `rule_decisions` (`plugins.rs:1948`) reads `Workspace::project_config(name)`, the root file only
   for a root project. In a repository whose top is not a project, no call carries the root file's
   decisions. The WIT comment says so: "the root file in the root project's call"
   (`wit/gate.wit:110-112`).
2. **The root exceptions file is read by no ledger rule.** The ledger rules are project scope (no
   `SCOPE` in `plugins/ledger/src/rules/*.rs`), so they run only in project calls. A project's call
   reads its `exceptions` option relative to the project's root, and `check_path`
   (`plugins/ledger/src/settings.rs:103`) refuses a path that is not in normal form, so a member
   project cannot name `../gate.exceptions.toml`. At a non-project root, the root exceptions file
   holds only repository-scope rules' exceptions: `workspace.rs:504-515` refuses a project-scope
   rule's exception there.
3. **The policy still requires the decision.** `Workspace::decision_policy`
   (`workspace.rs:255`) applies the root's `require-decision` to the root file's entries and to the
   root exceptions. At load, the host requires that the decision is *written*, never that it is
   *accepted*.
4. **The root cannot turn a ledger rule on for itself.** At a root that is not a project, a
   project-scope rule in the root `[rules]` is a configuration error: "is a project-scope rule; the
   root [rules] holds repository-scope rules only, set it for projects in [[profiles]]"
   (`Workspace::validate_except`, `workspace.rs:436`). T-071 skips a root preset's project-scope
   rules there. So the ledger rules can only run in member projects' calls, which never see the root
   file.

So with a root that is not a project:

```toml
# gate.toml
[workspace]
projects = ["apps/*"]

[plugins]
git = { source = "ElsiKora/Gate:git@0.3.0" }
ledger = { source = "ElsiKora/Gate:ledger@0.3.0", options = { decisions = "apps/docs/decisions/index.md" } }

[decision-policy]
require-decision = true

[rules]
"git/no-ci-skip" = { level = "off", decision = "D-CI-001" }

[[profiles]]
projects = ["apps/*"]

[profiles.rules]
"ledger/rule-decisions" = "error"
```

and `D-CI-001` `proposed` in the index, `gate check` exits 0. `ledger/rule-decisions` runs in each
member project's call, and those calls carry only that project's own decisions. Moving the entry to
the root `[rules]` is the configuration error of point 4. The first end-to-end test below fails
today on exactly this tree: it expects exit 1 and the finding at `gate.toml:<line>`.

## Approval

The lead approved Option 1 with these conditions:

- The three root rules share the judgement code of their project-scope counterparts: one function
  each, called from both scopes, no copied logic.
- `config-file` in `check-request` is documented in `wit/gate.wit` and the SDK; a rule that ignores
  it is unaffected.
- Order: the stack, then T-080, then T-072. Code starts on the stack tip; T-072 rebases onto
  `main` after T-080 merges and takes its WIT changes as they are. T-080 adds nothing to
  `check-request` (it reads the existing `rule-run.exceptions`); it changes what `check` returns
  (`check-result` with `judged`), `CallOutcome` and the result cache format, so `config-file` meets
  it only where the rebase joins the two.
- `ledger/recommended` and `ledger/adoption` (T-069) take the three rules; the preset tests still
  pass for a single-project repository, with nothing reported twice.
- Docs: the configuration guide (where a monorepo root writes its decisions and exceptions), the
  three rule pages, and the changelogs under 0.3.0.
- One lane round on the final base, then an independent review.

## Option 1 (approved): the repository call owns the root file when the root is not a project

The repository scope's own configuration file is the root `gate.toml` whenever the root is not a
project. Today that holds for everything the root writes except its decisions. So the request
follows the same placement:

- **Host.** The repository call carries the root file's decisions when the top is not a project.
  Under a root project, nothing changes: the root project's call carries them, and the repository
  call none. A member project's decisions never go to the repository call. `rule_decisions` takes
  an `Option<&str>`, and the WIT comment says "the root file in the root project's call, or in the
  repository call when the root is not a project".
- **Request: one fact the plugin needs.** The plugin cannot tell from the repository call whether
  the root is a project, so it cannot tell whether the root `gate.exceptions.toml` belongs to the
  repository scope or to the root project. Under a root project that file is the project's, and the
  project-scope rules already read it. `check-request` gains `config-file: string`, the `gate.toml`
  whose entries this call's `rule-decisions` come from, relative to the scope's root. It is
  `gate.toml` in every project call, `gate.toml` in the repository call when the root is not a
  project, and empty in the repository call under a root project. The SDK adds
  `run.config_file() -> Option<&str>`. It names no plugin and no layout: it is where the scope's own
  entries are written.
- **Ledger: three repository-scope rules**, the same judgement as the project rules
  (`index::refusal`, the same messages, keys and aspects), reading the root's entries:
  - `ledger/root-rule-decisions`: `run.rule_decisions()` of the repository call.
  - `ledger/root-exception-decisions` and `ledger/root-exception-owners`: the `[[exception]]`s of
    the exceptions file beside `run.config_file()` (option `exceptions`, default
    `gate.exceptions.toml`). When `config_file()` is `None` they check nothing.
  - All three read the index and the records through the root's `[plugins.ledger] options`
    (`decisions`, the ledger paths), relative to the repository root, under the repository grant,
    which reads the whole tree.
- **Presets.** The three rules go into `ledger/recommended` (T-069). A monorepo root that writes
  `[workspace] extends = ["ledger/recommended"]` takes exactly these three, since T-071 skips a root
  preset's project-scope rules there. A single-project repository's `[project] extends` takes both
  scopes. There the three rules see `config_file() == None`, so they check nothing, and the root
  file is checked once, by the project rules as today.

Why: the root's entries are checked by the same judgement and land where the reader looks, at
`gate.toml:<line>` and `gate.exceptions.toml:<line>` in repository paths, under the root's own
exceptions and baseline. No project has to be named as the ledger's home, and the core still names
no plugin.

Costs: three more rule ids, each with its page and tests; one more field in an interface that is
still unreleased.

## Option 2: carry the root's entries to the project that holds the ledger

A new `[workspace]` key names the project whose call checks the root's entries
(`ledger-project = "docs"`). The host adds the root file's decisions to that project's call, and
grants it the root `gate.toml` and `gate.exceptions.toml`. The existing project rules check them, so
no new rule ids are needed.

Against it:

- A plugin addresses every path relative to its scope's root. The root files are above that
  project, so the request would carry `../gate.toml`, which `check_path` refuses and which breaks
  the rule that a call sees only its scope's paths. Otherwise the runtime needs a second path space.
- The findings about the root files become that project's findings: their exceptions and baseline
  entries go in the project's files, not beside the root file they are about.
- The core learns that one project is "the ledger's", a key about one plugin's job.
- A monorepo without a docs project, or with two ledgers, has no answer.

## Not taken

- A rule declared for both scopes (`Scope::Both` in the SDK and WIT). The three project rules would
  run in the repository call too, without new ids. But scope also decides where a rule's exceptions
  live (`workspace.rs:504`), how its findings are keyed and where the root's presets apply (T-071).
  A third scope changes all of them for three ledger rules.
- Delivering the root's exceptions through the request, as T-059 declined for project files:
  `exception-owners` reads `owner` from the file either way.

## Tests

What would let a broken change through, or block a good one.

Host, through the runtime's probe component: a repository-scope probe rule `probe/root-decisions`
reports each decision of its call and the call's `config-file`, beside the project-scope
`probe/decisions` (`crates/gate/tests/plugins.rs`):

- Non-project root: a decision on a root `[rules]` entry arrives in the repository call, with the
  key as written, the path `gate.toml` and the entry's line, and with `config-file` `gate.toml`.
  No member project's call carries it.
- Root project: the repository call carries no decision and an empty `config-file`. The root
  project's call carries the root file's decisions with `config-file` `gate.toml` (T-059's
  placement, unchanged).
- A member project's decisions never arrive in the repository call; its own call says
  `config-file` `gate.toml`, relative to its root.
- A changed root decision changes the repository call's request, so the result cache does not
  answer it.

Ledger (`plugins/ledger/tests/rules/`), for each of the three rules:

- Pass: an accepted decision gives no finding; an exception without `decision` is not the
  decision rule's finding.
- Fail: an unlisted decision gives `#unlisted` at its line, and a `proposed` one gives
  `#not-accepted` naming the status. An index that is absent reads as listing nothing. An owner that
  is not an open implementation record is the owner rule's finding, as for a project.
- `config_file() == None`: an exceptions file at the root gives no finding.

End to end, with the built `git` and `ledger` components (`crates/gate/tests/plugins.rs`):

- The tree of "Current behaviour": exit 1 with `ledger/root-rule-decisions` at `gate.toml:<line>`
  while the index says `proposed`, and exit 0 once it says `accepted`. This is the test that fails
  today.
- The same for an `[[exception]]` of a repository-scope rule in the root `gate.exceptions.toml`,
  through `ledger/root-exception-decisions`.
- A single-project repository with `ledger/recommended` in `[project] extends` and one unlisted
  decision in its `gate.toml`: exactly one finding, from `ledger/rule-decisions`, and none from the
  root rules.

Each new rule's page, with its example finding run against a synthetic tree.

## Done when

- The lead approves one option.
- After approval, and once the stack is on `main`: the tests above pass in one lane round on the
  pushed tip (fmt, clippy, test, deny, release build), the leak scan prints nothing, and the pages,
  `docs/guide/configuration.md` (decisions and exceptions at a workspace root) and `CHANGELOG.md`
  say what changed.

## Resume point

2026-11-21T03:32:42+00:00: lane ticket 411 (`round1.sh`, one round on `8ed4f919`, on `main`
`6b36eeac`) exited 0: fmt, clippy with `-D warnings`, the ten new or changed `gate` end-to-end tests
and the ledger root, presets and presence tests by name, then `cargo test --workspace` (1515 passed,
0 failed, 7 ignored), `cargo deny check` and the release build. `gate-sdk`, `gate-runtime`,
`gate-host`, `gate` and `gate-plugin-ledger` compiled from this worktree only. Next: the independent
review; T-072 merges after T-080.

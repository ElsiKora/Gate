<!--
The lead writes a version's section before tagging it, newest first, in the format of the other
ElsiKora repositories; the release takes that section as its notes and fails without it:
a minor or major release is an `#` heading, a patch an `##` heading, each with `### Features` and
`### Bug Fixes` entries of the form `- **scope:** subject ([sha](commit link))`.
Entries link their commit from the second release on; the first release's section, 0.1.0, links
none, because the public history starts at a fresh root at 0.1.0.
The ledger plugin is versioned on its own; its releases are `plugin-ledger-v<version>`.
-->

## [0.3.3](https://github.com/ElsiKora/Gate/compare/plugin-ledger-v0.3.2...plugin-ledger-v0.3.3) (unreleased)

Built for the plugin interface `gate:plugin@0.3.0`. ledger 0.3.3 needs core 0.3.4, and core 0.3.4
needs ledger 0.3.3: upgrade both together. On a synthetic root project with a nested project, ledger
0.3.3 on core 0.3.3 judged only the nested file, which has a decided exception, and missed the
completed owner in the root file; ledger 0.3.2 on core 0.3.4 reported each of the 3 exceptions that
name no decision as an unlisted decision. The mismatch is harmless when every exception names a
decision, as under `[decision-policy] require-decision`.

### Breaking Changes

- **ledger:** `ledger/exception-owners` is repository-scope and judges the owner of every exception
  of every `gate.exceptions.toml` of the workspace once, at its own file and line, whether or not the
  root is a project. It replaces `ledger/root-exception-owners`, which is gone: use
  `ledger/exception-owners` in the root `gate.toml`, and remove every project-level entry for it,
  where a repository-scope rule is a configuration error. Its option `exceptions` is gone too: the
  rule takes the files the core reads. `ledger/recommended` and `ledger/adoption` turn it on at the
  level the two had. Set the merged rule at the root at the level your projects ran
  `ledger/exception-owners` at, not at the level of a root `ledger/root-exception-owners`: a root
  `off` now turns owner checks off for the whole workspace, every project's exceptions included.

### Bug Fixes

- **ledger:** the owner an exception of a nested project names was looked up in the implementation
  records at a path read inside the nested folder. It is now looked up in the records the root's
  `[plugins.ledger]` names, relative to the repository root.
- **ledger:** `ledger/config-decisions` skips an exception that names no decision, which the core
  now lists with an empty one.

## [0.3.2](https://github.com/ElsiKora/Gate/compare/plugin-ledger-v0.3.1...plugin-ledger-v0.3.2) (2026-11-26)

Built for the plugin interface `gate:plugin@0.3.0`. ledger 0.3.2 needs core 0.3.3 or later, and core
0.3.3 needs ledger 0.3.2: upgrade both together. With core 0.3.2, ledger 0.3.2 checks no decision
under a root project, and only the root gate.toml's rule entries under a root that is not a project.
With ledger 0.3.1, core 0.3.3 reports each exception's decision twice.

### Breaking Changes

- **ledger:** one repository-scope rule, `ledger/config-decisions`, judges every decision the
  workspace's configuration writes, once, against the index of the root's `[plugins.ledger]`: the
  rule entries, families and `[[profiles]]` rules of the root `gate.toml` and of every project's
  `gate.toml`, and the exceptions of every `gate.exceptions.toml`, each at its own file and line. It
  replaces `ledger/rule-decisions`, `ledger/root-rule-decisions`, `ledger/exception-decisions` and
  `ledger/root-exception-decisions`, which are gone: use `ledger/config-decisions` in the root
  `gate.toml`, whether or not the root is a project. `ledger/recommended` and `ledger/adoption`
  turn it on at the level the four had. ([376a9c9](https://github.com/ElsiKora/Gate/commit/376a9c976b8fbaf8a8438cdb637b03381b61ad6c))
- **ledger:** `ledger/config-decisions` and `ledger/root-exception-owners` are repository-scope, so
  a project that turns them on, by extending `ledger/recommended` or `ledger/adoption` or with a
  `ledger/*` family, needs the root to name them too: under core 0.3.3 a project-only ledger is a
  configuration error. Declare `[plugins.ledger]` in the root `gate.toml`, with the index by its
  repository path, and turn the rules on there, or set them `off` there on purpose. ([376a9c9](https://github.com/ElsiKora/Gate/commit/376a9c976b8fbaf8a8438cdb637b03381b61ad6c))

### Bug Fixes

- **ledger:** a decision named in a nested project's `gate.toml` or `gate.exceptions.toml` was
  judged against an index path read inside the nested folder, so every one, valid or not, was
  reported as unlisted. It is now judged against the workspace's index, at its own file and line. ([376a9c9](https://github.com/ElsiKora/Gate/commit/376a9c976b8fbaf8a8438cdb637b03381b61ad6c))

## [0.3.1](https://github.com/ElsiKora/Gate/compare/plugin-ledger-v0.3.0...plugin-ledger-v0.3.1) (2026-11-23)

Built for the plugin interface `gate:plugin@0.3.0`; it loads in a 0.3.0, 0.3.1 or 0.3.2 core. The
findings are the same as 0.3.0's; only the time changes.

### Performance

- One catalog and one record read per call: each ledger file is read and fenced-blanked once, and
  the ledger's records are computed once, shared by every rule of the call instead of once per
  rule. On a large tree the plugin's fuel went from 38.83e9 to 21.04e9 (-45.8 %) and its host reads
  from 13020 to 11585. ([e8eade1](https://github.com/ElsiKora/Gate/commit/e8eade14021b9eaf80cdb40dd94f1b5592e1c696))
- Its regular expressions run on `regex-automata` built with its faster engines (the lazy DFA,
  the bounded backtracker, the one-pass DFA and literal prefilters), which report the same matches.
  ([45eda36](https://github.com/ElsiKora/Gate/commit/45eda36e3e5c54bb11ab6885169898c87b76df34))

# [0.3.0](https://github.com/ElsiKora/Gate/compare/plugin-ledger-v0.2.0...plugin-ledger-v0.3.0) (2026-11-21)

Built for the plugin interface `gate:plugin@0.3.0`; it loads only in a 0.3.0 core.

### Breaking Changes

- **ledger:** an enabled rule fails when a file it reads is not tracked, instead of reporting
  nothing: each such file is a finding of the new aspect `input` of every rule that reads it, at
  the file's path and keyed by it, naming the option that points there. The manifest itself is
  `ledger/manifest#input`. A project without its ledger now fails; add the files, point the options
  at them, or set `levels = { input = "off" }` with a decision.
- **ledger:** options without which an enabled rule did nothing are required, and leaving one out
  stops the run with exit 2: `field` of `ledger/closing-notes` and `catalog` of `ledger/reuses`
  have no default, and `ledger/record-status` needs the shared `parity`.

### Features

- **ledger:** the presets `ledger/recommended` and `ledger/adoption`: `decisions`, `manifest`,
  `records`, `blockers`, `exception-owners`, `exception-decisions`, `rule-decisions`,
  `root-exception-owners`, `root-exception-decisions` and `root-rule-decisions`, at `error` and at
  `warning`.
  They leave out `closing-notes`, `reuses` and `record-status`, which need the project's `field`
  and `catalog` and the plugin option `parity`.
- **ledger:** rebuilt for the plugin interface `gate:plugin@0.3.0`, whose `generate` call also
  passes the `[plugins.ledger]` options and whose `check` call carries the rule decisions of the
  scope's own `gate.toml`. Its generators and the files they write are unchanged; its rules change
  as the entries of this section and of Breaking Changes say. Pin `ledger@0.3.0` with the 0.3.0
  core.
- **ledger:** `ledger/rule-decisions` reports a decision a rule entry names, in `[rules]` or in a
  profile's `rules`, that the decision index (`decisions`) does not list as accepted, as
  `ledger/exception-decisions` does for exceptions. It reads the decisions the core hands it, never
  `gate.toml`.
- **ledger:** the generator `layout` seeds an empty ledger at the paths of
  `[plugins.ledger] options`: the manifest (the five shared files under their common root, no
  identifier family, no forbidden reference) and the five shared files, each with a title, the
  decision index with the block of `ledger/decision-counts` and its table header; with a tracked
  manifest, also each canonical file it lists. Every file is a seed, written only where nothing
  exists, and a tracked file is never seeded.
- **ledger:** three repository-scope rules hold the root files of a workspace whose root is not a
  project to the same judgement as a project's own: `ledger/root-rule-decisions` for the decisions
  of the root `gate.toml`'s entries, `ledger/root-exception-decisions` and
  `ledger/root-exception-owners` for the decisions and owners of the root `gate.exceptions.toml`.
  Their options are read relative to the repository root. Under a root project they report nothing,
  since the project rules check the root files there.

# [0.2.0](https://github.com/ElsiKora/Gate/compare/plugin-ledger-v0.1.0...plugin-ledger-v0.2.0) (2026-11-19)

Built for the plugin interface `gate:plugin@0.2.0`; it loads only in a 0.2.0 core.

### Features

- **plugin-ledger:** the ledger rules (records, record status, decisions, manifest, reuses, closing notes, blockers,
  exceptions and the review-progress generator) give each finding whose message names a line, a
  path or an option value a stable key (what it is about, never a line or column), so an edit
  above a finding or a reworded message keeps its fingerprint. These findings' fingerprints change
  once, with this release; `gate baseline prune` re-keys their baseline entries when the
  repository upgrades. ([c688b5d](https://github.com/ElsiKora/Gate/commit/c688b5df1c1e379e4189f0b65accef02a1d3e883))
- **plugin-ledger:** a wrong option, or an exceptions file the options name that is not valid TOML, ends the run as a
  configuration error (exit 2, `configuration error`), not as an environment error. A file the rule
  cannot read stays an environment error. ([23a2719](https://github.com/ElsiKora/Gate/commit/23a2719a93bbdfdf9c4e61354c20a702d7501339))

# 0.1.0

### Features

- **plugin-ledger:** the rules `blockers`, `closing-notes`, `decisions`, `exception-decisions`,
  `exception-owners`, `manifest`, `record-status`, `records` and `reuses`, with the shared
  `[plugins.ledger] options`, whose schema the plugin declares so the core checks them when the
  plugin loads.
- **plugin-ledger:** the generators `decision-counts` and `review-progress`, which write the counts
  of a decision index and the review progress of a roadmap into their managed blocks.

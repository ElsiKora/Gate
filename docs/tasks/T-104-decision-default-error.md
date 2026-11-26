# T-104: a rule no preset names has the default `error` for the decision policy

| Field | Value |
| --- | --- |
| Status | done (reviewed, one fix round; gates green on `4b40022`; merged) |
| Release | the next core release (the plugin interface stays `gate:plugin@0.3.0`) |
| Writer | "Gate T-104" |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `80db512` |
| Touches | `crates/gate-host/src/config/` (where `[decision-policy] require-decision` compares a level with its default), its tests, `docs/guide/configuration.md` ("Decision policy"), `docs/architecture.md` where it states the default, `docs/plan.md`, this task file |

## Goal

Under `[decision-policy] require-decision = true`, a `[rules]` entry below its default must name a
decision. Today the default is only what a preset gives. A rule that no preset names "has no
default ... so it needs no decision at any level" (`docs/guide/configuration.md`, "Decision
policy"). So a configuration can lower such a rule from `error` to `warning`, `note` or `off`, and
the policy does not notice.

The first consumer runs 18 rules that no preset names, most of them with options that are its own
(`nestjs/owner-roots`, `nestjs/package-pins`, `git/main-history` and others). A preset cannot name
a rule that needs options the plugin cannot know. So the gap is closed in the core, for every rule,
rather than by adding the rules to presets.

## Design (the lead's decision)

1. **The default of a rule, or of an aspect, that no preset the scope extends names is `error`.**
   This is the level the configuration resolves to when nothing sets one ("else it is `error`" in
   the level order of `docs/guide/configuration.md`). The policy and the level resolution now use
   the same default. A preset's level still wins where a preset names the rule or the aspect.
2. Therefore, under `require-decision = true`, an entry or a family (`"<plugin>/*"`) entry that sets
   such a rule, or any of its aspects in `levels`, to `warning`, `note` or `off` needs `decision`.
   `error` needs none. The error message is the one used for a preset-named rule, naming the
   default as `error`.
3. Without `[decision-policy]`, nothing changes.
4. The guide's paragraph "A rule no preset names has no default ..." is replaced by the new rule,
   with one example of an unnamed rule lowered with a decision. The architecture document says the
   same in one place.
5. No option, no switch, no compatibility path: there is one way.

## Proof

- Tests, at the load of a configuration with `require-decision = true`:
  - an unnamed rule at `warning`, at `note` and at `off`, each without `decision`: refused, exit
    `2`, with the message naming the rule, its default `error` and what to add;
  - the same with `decision`: accepted;
  - an unnamed rule at `error` without `decision`: accepted;
  - an unnamed rule's aspect lowered in `levels`: refused without `decision`;
  - a family entry below `error` over a plugin with an unnamed rule: refused without `decision`;
  - a preset-named rule: unchanged behaviour;
  - no `[decision-policy]`: unchanged behaviour.
- `bin/gate check` on this repository passes with `gate.toml` as it is, or the task says exactly
  which entry needed a decision and why.
- All cargo runs go through the lane. `cargo fmt --check`, `clippy -D warnings`, `cargo test` and
  `cargo deny` are green.

## Notes

- The check lives in `crates/gate-host/src/plugins.rs` (`check_decisions`, `Floor`, `decided`),
  not under `config/`: presets come from plugins, so rule entries are checked when the plugins load.
- The same default applies to an aspect of a preset-named rule when the preset sets no level for
  that aspect and none for the rule (the level order gives it `error`). Seen with the probe's
  `probe/quiet`, which sets `aspects#quiet` only: `levels = { loud = "note" }` and a whole-rule
  `warning` now need a decision. A level a preset sets is still met first, so every message for a
  preset-set level is unchanged.
- A family is reported against the first rule of the plugin, in declared order, whose default is
  above the family's level and which has no whole-rule `level` of its own. The existing family test
  over `probe/lenient` now names `probe/read` (the probe's first rule, which no preset names)
  instead of `probe/aspects`.
- Fix round (the lead's decision): the family counts. The default of a level a rule entry writes
  (its `level`, or one in `levels`) is the level the scope resolves to without the entry, in the
  documented order: the rule's family, the preset's aspect, the preset's rule, then `error`. So a
  rule or an aspect raised above an `off` family needs no decision, and one below a `warning`
  family does. The family's own level is still met with the presets' default, or `error`, without
  other families. The message names the family: `(the family "<plugin>/*" sets <rule>[#<aspect>]
  to <level>)`.
- The message for the new default: `... sets <rule> below its default error (no preset the scope
  extends names <rule>)`, or `(no preset the scope extends sets a level for <rule>[#<aspect>])`
  when a preset names the rule.
- This repository's `gate.toml` has no `[decision-policy]`, and `bin/gate` runs the released core,
  so `bin/gate check` is not affected by the change.

## Results

The fix round's lane round on `4b40022` (2026-11-26T08:08:04+00:00 to 2026-11-26T08:27:37+00:00),
in order:

| Command | Exit |
| --- | --- |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --locked -j 4 --all-targets -- -D warnings` | 0 |
| `cargo test --locked -j 4` | 0: 1655 passed, 0 failed, 7 ignored, over 124 test binaries |
| `cargo deny --locked check` | 0: advisories, bans, licenses and sources ok |

The first round, on `0be04c8` (2026-11-26T02:30:21+00:00 to 2026-11-26T02:53:22+00:00): the same
four commands each exited 0, with 1654 tests passed, 0 failed, 7 ignored.

`bin/gate check` on this repository: exit 0 (7 rules, 0 errors, 0 warnings), with `gate.toml` as
it is; it has no `[decision-policy]`, so no entry needed a decision.

## Resume point

Done; the commit after `4b40022` changes only this task file and `docs/plan.md`. Next: the lead's
decision and merge.

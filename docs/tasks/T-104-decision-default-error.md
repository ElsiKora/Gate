# T-104: a rule no preset names has the default `error` for the decision policy

| Field | Value |
| --- | --- |
| Status | open |
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

## Resume point

Not started.

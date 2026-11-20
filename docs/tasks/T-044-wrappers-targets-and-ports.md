# T-044: `nestjs/wrappers` names own-parameter reads and adapters of interface ports

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.3.0 (plugin nestjs 0.3.0 with the core, on `gate:plugin@0.3.0`; merges after the v0.2.1 tag) |
| Writer | "Gate 002", from 2026-11-20 (first "Gate 003: host foundations") |
| Reviewer | a session that did not write it |
| Depends on | `main` at `a8e511d` (v0.2.1 and the first 0.3.0 batch) |
| Touches | `plugins/nestjs/src/rules/semantics/wrappers/`, `plugins/nestjs/tests/wrappers.rs`, `plugins/nestjs/rules/wrappers.md`, `plugins/nestjs/CHANGELOG.md` |

## Goal

Two general changes to `nestjs/wrappers` that a consumer asked for.

1. A raw Nest exception construction whose arguments read a plain parameter of the scope's own
   function gets the scope form of the target, which an exception can name, instead of the line
   form, which moves with every edit above it.
2. A reporter adapter may implement an interface port as well as extend a class port: its
   forwarding method is a framework boundary either way.

## Scope

1. `raw-nest-exception` target: the plain parameters (no default, no destructuring, not
   reassigned, not redeclared) of the scope's own function join the `parameters <name>` list, in
   declaration order with those of an inner callback, for example
   `src/x.ts: findListed parameters baseUrl: new BadRequestException({ ... })`. A read of a local
   variable, of a reassigned parameter or of `arguments` keeps the line form.
2. `reporter-adapters`: an entry matches a class below `path-prefix` that extends the named export
   or implements it. The field `class` becomes `port`, since an interface is no class; no alias.
   The CHANGELOG says so.
3. The rule page's target section and option table say both.

Not in scope: any other aspect of the rule; nestjs speed work (T-037 runs in parallel).

## Done when

- Tests: an own-parameter read gets the scope form and a reassigned parameter keeps the line form;
  an adapter implementing the port has its forwarder pass, and a class implementing another
  interface still fails. Each fails with its change taken out.
- `cargo fmt`, `clippy -D warnings`, `cargo test`, `cargo deny` pass in one lane round.
- The leak scan prints nothing.

## Resume point

2026-11-19T19:27:00+00:00: the release moved to v0.3.0: every plugin of the repository releases at 0.3.0
with the core on `gate:plugin@0.3.0` (T-057), so this breaking change merges after the v0.2.1 tag.
The plugin CHANGELOG entry is under `# 0.3.0 (unreleased)` / `### Breaking Changes`. Lane round
232 on 9971f1a counts for the code; this follow-up is documents only.
2026-11-20T10:14:04+00:00: Gate 002 takes over as writer. The branch is rebased onto `main` at
`5da3272` (v0.2.1); the one conflict was the plugin CHANGELOG, whose `# 0.3.0 (unreleased)` / `###
Breaking Changes` section now sits above the released 0.2.1 section. Lane round 208 on `0576b2b`
(the `feat` commit) failed one test,
`own_parameters_are_named_and_reassigned_or_defaulted_ones_and_locals_are_not`, on the branch's own
assertion (that the scope form drops the finding); `3262226` ("the scope form stays put when lines
are added above") changed it to what the scope form is for, an unchanged target when a line is added
above. Rounds 208 and 232 built into another task's target directory, so neither counts. Next: one
lane round on T-044's own target.
2026-11-20T10:22:46+00:00: rebased again, onto `main` at `a8e511d`, the first 0.3.0 batch, as the
lead asked; it applied without a conflict, and the batch changed nothing under `plugins/nestjs` (its
SDK change is the generators' plugin options and seeds). Round 341 on `b06cb87` was stopped in
clippy for the new base. Next: one lane round on T-044's own target.

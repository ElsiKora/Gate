# T-044: `nestjs/wrappers` names own-parameter reads and adapters of interface ports

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.1 |
| Writer | "Gate 003: host foundations" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `v0.2.0` |
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

2026-11-19T17:34:26+00:00: both changes, their tests, the rule page and the CHANGELOG are committed;
not compiled yet. Next: one lane round (fmt, clippy, test, deny) and each change taken out with its
tests run (each must fail).

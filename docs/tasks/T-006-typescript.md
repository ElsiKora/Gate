# T-006: TypeScript language library and plugin

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 001" |
| Reviewer | a separate review session |
| Depends on | T-003 for the plugin half (the library half starts now) |
| Touches | `languages/typescript/**`, `plugins/typescript/**`, root `Cargo.toml` members and `Cargo.lock` |

## Goal

The reference implementation's TypeScript layer becomes `gate-typescript`, a library any rule plugin
links; and the `typescript` language plugin declares the language and reports files that do not
parse.

## Scope

1. Library (`languages/typescript`, crate `gate-typescript`): move the reference implementation's
   TypeScript and JavaScript layer, and the shared helpers its rule packs duplicate (outside this
   repository; the white-space set and the expression unwrapper exist in several copies: one each now). It reads
   files through a small trait (`Source`: `paths`, `text`) that `gate-sdk`'s `Tree` implements, so the
   library does not depend on the runtime. tsconfig path aliases and the skip folders come from
   `[languages.typescript]` settings (a typed struct), not constants. Keep the behaviour exactly; this
   is a move with the product-bound constants turned into settings.
2. Plugin (`plugins/typescript`): language `typescript` (`.ts .tsx .mts .cts .js .jsx .mjs .cjs`), rule
   `typescript/parse` (every tracked source file parses; the reference's message), built with
   `gate-sdk` once T-003 pushes its surface; fixtures with `gate-test`.
3. Your port brief with the rule's options, messages and the reference's file:line facts is the private brief your
   session brief names: nothing from it is copied into the repository; write synthetic fixtures.

## Done when

- The library's tests (moved and new, synthetic sources) pass; fmt, clippy, test, deny exit 0 (oxc
  builds through the queue).
- The plugin builds as `wasm32-wasip2`, its `gate-test` fixtures pass (one parsing and one failing
  file per extension class).
- Acceptance outside the repository (the acceptance harness): `typescript/parse` reports what the
  reference reports on the three trees and their mutations.
- Leak scan prints nothing.

## Resume point

2026-11-17T16:37:26+00:00: merged to main; the lead's gates on the merged tree all exit 0.

Library and plugin written on `task/T-006-typescript`, review fix round applied, replayed onto main
`38ca5de` (T-003 merged) with `gate-sdk` as the workspace crate; checks and acceptance passed on main's
host. Left: the merge.

- `languages/typescript` (crate `gate-typescript`): `Source` (`paths`, `bytes`, and provided `text` and
  `contains`) is the only way in. `MemorySource` holds files in memory, and `ReadSource` reads through a
  function, which is how a plugin bridges its SDK tree without the library depending on the SDK.
  `#![deny(missing_docs, clippy::expect_used)]`; every public item is documented.
- `Source` has `bytes` besides `text`: `typescript/parse` reports a file that is not UTF-8 as fatal,
  which needs the raw contents.
- `Settings` is `[languages.typescript]`: `skip-folders` (default `.git`, `build`, `coverage`, `dist`,
  `node_modules`) and `tsconfig` (default `tsconfig.json`), kebab-case, unknown keys rejected. A
  configured `tsconfig` other than the default that is not tracked is an environment failure; a missing
  default disables `paths`. `TypeScript::new(source, settings, allocator)`;
  `owners::Sources::new(typescript, root)` takes the source root as a parameter, and an empty root means
  the whole source.
- The resolver mounts the source under `/gate/project`; a specifier whose `..` leaves the project
  resolves to nothing, and error texts name paths relative to the project.
- The two framework package constants of `syntax.rs` stay with the plugin that names the framework.
- The expression unwrappers: `value::unwrap` (parentheses and assertions) and `owners::unwrap` (also a
  non-null assertion closing an optional chain) differ on purpose; one copy of each.
- `plugins/typescript` (package `gate-plugin-typescript`, component `typescript.wasm`): rule `parse`
  (page `plugins/typescript/rules/parse.md`), language `typescript`, option `extensions`, settings read
  with `run.language::<Settings>("typescript")`. A finding sits at the first error's line and column; a
  file that is not UTF-8 is reported at the file without a line; the target is the path. `gate-sdk` is
  the workspace crate `crates/gate-sdk`.
- `deny.toml` carries the oxc exceptions: the `dragonbox_ecma` licence exception, exact-version skips
  for `hashbrown@0.14.5`, `itertools@0.14.0` and `num-bigint@0.4.8`, and the oxc reasons added to the
  `syn@2.0.119` and `hashbrown@0.16.1` skips. `Cargo.lock` keeps every entry of main's lock and the oxc
  dependency versions of the baseline binary; the four `icu_locale_fallback*`/`icu_segmenter*` crates
  are at 2.4.0, the version main's `icu_collections` 2.3 admits.
- Checked 2026-11-17 on `62dbe58` (the full round, queued): `cargo fmt --all --check` exit 0; `cargo
  clippy --locked --all-targets -- -D warnings` exit 0; `cargo clippy --locked -p gate-plugin-typescript
  --target wasm32-wasip2 -- -D warnings` exit 0; `cargo test --locked` exit 0 (278 passed); `cargo deny
  --locked check` exit 0; `cargo build --release --locked` exit 0; `cargo build --release --locked -p
  gate-plugin-typescript --target wasm32-wasip2` exit 0 (1014672 bytes); leak scan prints nothing.
- Acceptance ran outside the repository on 2026-11-17 with that round's host and component, harness
  release `cfa1ef1` without shaping flags (record kept with the other acceptance data):
  - A: 0 findings on each of the three trees, as the reference.
  - B: no exceptions apply; the plugin-only run equals A on the three trees.
  - C: the mutation case's one finding is identical.
  - D: no replayed case uses the rule. Gate and the reference agree on all 1,181 replayable trees (0 findings).

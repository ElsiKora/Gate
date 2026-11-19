# T-007a: Plugin nestjs, the crate and the structure rules

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | T-006 (the TypeScript library), T-003 (SDK surface) |
| Touches | `plugins/nestjs/{Cargo.toml,CHANGELOG.md,src/lib.rs,src/layout.rs,src/shared/**}`, `plugins/nestjs/src/rules/structure/**`, their rule pages and tests |

## Goal

The `nestjs` plugin crate and the first half of its rules, built on `gate-typescript`, reporting the same
findings as the reference implementation's checks they replace ([architecture.md](../architecture.md) sections 4
to 6). The crate skeleton lands first so that T-007b can add its rules beside it.

## Scope

1. First commit, pushed within two hours: the crate (`cdylib` for `wasm32-wasip2`, `gate_sdk::plugin!`),
   the `[plugins.nestjs] options` layout block (typed, schema generated), the shared helpers both halves
   need, and an empty `rules/structure` and `rules/semantics` module each with a registration list, so
   T-007b only adds files under `rules/semantics/`.
2. The rules of group B in the private port brief (the local rules file names where the briefs are),
   plus `nestjs/notification-egress`; options, aspects, target grammars as the brief lists them.
3. A page per rule; tests per rule with a test double host: each pass, each fail, each aspect, option
   errors. Fixtures are synthetic.

## Done when

- fmt, clippy (native and `wasm32-wasip2`), test, deny exit 0; the plugin builds as a release component.
- Acceptance outside the repository with the acceptance harness: zero findings on the clean reference
  trees, identical findings on the mutation and replayed cases, for every rule
  of this half.
- The leak scan named in the local rules prints nothing.

## Resume point

`task/T-007a-nestjs-structure` is replayed onto T-006 `62dbe58` (the code main merged as T-006), has
main merged in up to `6383f4a`, and uses the workspace `crates/gate-sdk`.

Done, each with its page and tests: `adapter-boundary`, `advisory-lock-boundary`,
`advisory-lock-manifest`, `asset-storage`, `automator-metadata`,
`barrel-exports`, `constant-files`, `controller-owners`, `declaration-files`, `dto-decorators`,
`enum-values`, `error-logging`, `field-inventory`, `import-direction`, `notification-egress`,
`package-pins`, `raw-config-access`, `runtime-factory`, `string-enums`.
Rounds (`cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`,
the same for `gate-plugin-nestjs` and `gate-plugin-typescript` on `wasm32-wasip2`, `cargo test
--locked`, `cargo deny --locked check`, `cargo build --release --locked`, the `wasm32-wasip2` release
build of `gate-plugin-nestjs`, leak scan): `b6889e3` every step exit 0 (420 tests passed); `f1ea842`
(main `40358b8`) every step exit 0 (438 passed); `139c153` (main `109c410`, the skip-list review)
every step exit 0 (534 passed); `d615bc4` (main `6383f4a`, the gitignore matching) every step exit 0
(623 passed). Counted acceptance of `d615bc4` (harness `6f5e924`, main's gate `6383f4a`): A 2 of 3
identical, C 89 of 91; the differences are the nested `coverage/` finding the anchored default brings
(accepted, lead, 2026-11-17) and a message naming the rule by its reference id. Messages name a rule by
its Gate id only: `field-inventory` no longer writes its reference id (lead, 2026-11-17). `02813bf` every
step exit 0 (623 passed); its part C on harness `345ad2c`: 88 of 91 identical, one expected, and two
`notification-egress` messages that printed the configured `policy-label`.
`notification-egress` has no `policy-label`: its messages name "the nestjs/notification-egress
policy" in fixed wording, and the key is an unknown field (lead, 2026-11-17). `ca12f81` every step
exit 0 (623 passed). Part D of `02813bf` on `345ad2c`: 330 of 601 identical; every group but
repository-contract identical; there, 1811 import-direction findings for contract folders the minimal
trees lack (the check is gone) and one accepted `infrastructure/cdk/dist` finding of declaration-files.
`64935a5` (import-direction binds nothing in a folder without files) every step exit 0 (623 passed).
Parts of `import-direction` alone, `64935a5` on harness `46650d7`: A 3 of 3, C 6 of 6, D 313 of 313
identical. Parts of `notification-egress` alone, `ca12f81` on harness `76cd2da`: C 4 of 4 identical, D
has no case. Merged into main 2026-11-18; the merge gates (fmt, clippy native and `wasm32-wasip2`,
test, deny, release builds) exit 0 with 623 tests passed, leak scan exit 0.
Review round: `Layout::source_prefix` and `layout::folder_prefix` join layout folders (`""` is the
project root); `PluginOptions::read` rejects a layout folder that is not a normalized path, and every
rule checks its own path options through `shared::paths` (the error names the option and shows the
normalized form; the SDK puts the rule's name before it). Every rule declares `typescript`, except
`package-pins` (none). Comments and pages no longer name the reference implementation or its functions.
`field-inventory` reads the sources through `Read` (the sources with the `Names` vocabulary: packages,
planes, column types, formats, depths); its findings carry targets, `unclassified` with the pinned
planes. The reference's baseline is gone (exceptions now), and so is the field report, which becomes the
generator `nestjs/field-report` in a later task.
`notification-egress` has required options (no container default). It and `field-inventory` read an
optional Markdown decision index through the tree with `shared::decisions` (a set path that is not
tracked stops the run) and locate entries with `shared::json_lines` (`element_lines`, `text_line`).
`automator-metadata` reports a stale option entry at the line of its key in the rule's options file,
or at `gate.toml` without a line when the options sit there; the test host takes an options file
through `support::run_from`. `shared::decorators` holds the decorator, object-literal and member
helpers and `shared::js` the JavaScript-value reading of lenient option rows (`controller-owners`).
`advisory-lock-boundary` keeps its source reconstruction in its own folder; its shell patterns use
`regex-automata` (no `regex` crate).
`enum-values` takes its pattern engine from `regex-automata` 0.4.18 (already locked through globset).
Second review round: `modules-root`, `infrastructure-folder` and `path-aliases` derive from
`source-root` when unset (`LayoutInput` into `Layout`), and so do the rule options below them
(asset-storage, advisory-lock manifest paths, field-inventory `contract-roots`); `rules/layout.md` lists
every key. The skip list is the TypeScript language's, matched one way (`Settings::skips`, as `.gitignore` matches a folder: a name without `/` at any depth,
an entry with `/` from the project root; default `.git`, `node_modules`, `/build`, `/coverage`, `/dist`;
lead, 2026-11-17); the plugin has no skip list of its own.
Which walks skip follows the reference: asset-storage, the advisory-lock-manifest markers, the entity
files automator-metadata lists and error-logging skip none (`tree::UNSKIPPED`); every other walk,
the infrastructure-as-code project's included, skips the language list. Options a rule cannot work without are required (advisory-lock-boundary `executor`,
import-direction, runtime-factory and raw-config-access lists); an untracked advisory-lock manifest
and field-inventory entries without a tracked decision index are configuration findings. An
import-direction source directory without files binds nothing and reports nothing, as in the
reference (lead, 2026-11-17). controller-owners and field-inventory rows are typed
kebab-case structs (messages keep their camelCase words). Shell patterns compile fallibly once per run.
Acceptance pairs this plugin (SDK `d4cc59b`) with a host built from `d4cc59b` and the harness's
`--ci-main-shape --drop-core-entries`: main's host changed the `git` range record after `d4cc59b`.

- `plugins/nestjs` (package `gate-plugin-nestjs`, lib `nestjs`, component `nestjs.wasm`).
- `layout.rs`: `PluginOptions { layout }`, read with `PluginOptions::read(run)` from
  `[plugins.nestjs] options`. Every key of the layout block is typed, kebab-case and optional, with the
  conventional defaults; unknown keys are rejected, and the schema closes the table over its keys.
  `Layout::resolve_alias` (equal or `alias/` prefix, the longer alias wins), `is_source_owner`,
  `planes_without_domain`.
- `shared/`: `project` (the run's tracked files as a TypeScript source, the `[languages.typescript]`
  settings, `parsed` skipping a file without a syntax tree), `tree` (walks below a root outside skipped
  folders, folders, entries, test support and scenarios), `modules` (`loads`, `relative_target`,
  `normalize`), `syntax` (`top_level_classes`, `typescript_sources`), `report::location`.
- Rule lists: `rules/structure/mod.rs` and `rules/semantics/mod.rs` each hold a macro that adds its
  rules to the list and passes it on; `lib.rs` declares the plugin with both. A group adds a rule by a
  `pub mod` line and a line in its own macro, without touching `lib.rs`.
- `canonical.rs`: `canonical_construction(&str) -> Option<String>`, a stub that answers `None`.


# T-007b: Plugin nestjs, the semantic rules

| Field | Value |
| --- | --- |
| Status | done |
| Release | v0.1.0-rc.1 |
| Writer | "Gate 006: nestjs semantic rules (T-007b)" |
| Reviewer | a separate review session |
| Depends on | T-007a's skeleton, on `task/T-007a-nestjs-structure`: branch from it |
| Touches | `plugins/nestjs/src/rules/semantics/**`, `plugins/nestjs/src/canonical.rs`, their rule pages and tests |

## Goal

The second half of the `nestjs` plugin's rules, added beside T-007a's in the same crate, reporting the
same findings as the reference implementation's checks they replace ([architecture.md](../architecture.md)
sections 4 to 6).

## Scope

1. The rules of group C in the private port brief (the local rules file names where the briefs are),
   plus `nestjs/module-registry`; options, aspects and target grammars as the brief lists them. Files
   only under `rules/semantics/`, registered in its list.
2. `canonical_construction(&str) -> Option<String>`, a pure public function in `src/canonical.rs` that
   prints a wrapper construction in the canonical form `nestjs/wrappers` uses in its targets, so
   exception targets can be written in that form.
3. A page per rule; tests per rule with a test double host: each pass, each fail, each aspect, option
   errors. Fixtures are synthetic.

## Done when

- fmt, clippy (native and `wasm32-wasip2`), test, deny exit 0; the plugin builds as a release component.
- Acceptance outside the repository with the acceptance harness: zero findings on the clean reference
  trees, identical findings on the mutation and replayed cases, for every rule
  of this half.
- The leak scan named in the local rules prints nothing.

## Resume point

2026-11-17. Branch `task/T-007b-nestjs-semantics` is on T-007a (main plus
T-007a) and holds three commits: "feat(plugin-nestjs): the semantic rules" (all 18 rules, their
pages and tests), a follow-up that makes `enum-folder` a single name and pins the unskipped walks
of prefix-nesting and migrations, and this task file. Merge order: T-007a first, then T-007b.

History: the 25 per-rule commits stopped compiling once rebased onto T-007a (T-007a removed the
layout skip lists they used), so the lead had the branch squashed into one code commit; every
commit compiles. Review rounds: the semantic review (fixed in the squashed commit), the skip-list
round (every walk through `shared::tree`; one list, `[languages.typescript] skip-folders`,
matched the `.gitignore` way; the style rules read only that list), and its verification (enum-folder
and the two walk tests, in the follow-up).

The rules: `nestjs/api-readiness`, `nestjs/custom-routes`, `nestjs/entity-descriptions`,
`nestjs/entity-table-name`, `nestjs/eslint-directives`, `nestjs/import-aliases`,
`nestjs/kind-layout`, `nestjs/migrations`, `nestjs/module-names`, `nestjs/module-registry`,
`nestjs/native-functions`, `nestjs/native-repository`, `nestjs/owner-boundaries`,
`nestjs/owner-roots`, `nestjs/prefix-nesting`, `nestjs/repeated-segment`, `nestjs/typeorm-relation`
and `nestjs/wrappers`. Which walks skip the language list and which read every folder follows the
reference; each page says which, and each walk has a test that fails if it used the other.

Counted acceptance, on the code (component sha256 `eb77fc61...2f44`), main's gate,
`--plugins-only`; `report.mjs` exit 0 on both result sets:
A 3/3; C 74/74; D 897/898 identical plus 1 expected (eslint-directives under a nested build output
folder: the style rules' own skip prefix is gone, no consumer tracks that folder); B 9/9, 130
identical plus 1 expected (T-007a's field-inventory in a nested `coverage/` folder the reference
skipped) and 290/290 on the three trees. Expected differences are the lead's
decisions, in the harness's list.

After the rebase the component changes only by field-inventory's messages (its Gate
id for the reference id); the follow-up changes behaviour only for an `enum-folder` that is not a
single name. C is rerun for that.

The lead read the follow-up and accepted it. Merged into main 2026-11-18 after T-007a: the merge gates on
the merged tree (fmt, clippy native and `wasm32-wasip2`, test, deny, release builds) exit 0 with 825
tests passed, leak scan exit 0.

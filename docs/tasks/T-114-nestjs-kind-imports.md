# T-114: `nestjs/kind-imports`, the import form of kind-folder barrels

| Field | Value |
| --- | --- |
| Status | in progress (requested by the first consumer for two products; no date asked) |
| Release | the next `nestjs` plugin release, with T-115 |
| Writer | assigned, branch `task/T-114-nestjs-kind-imports` |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `faf776e` |
| Touches | `plugins/nestjs/` (a new rule under `src/rules/`, its page, `src/lib.rs`, `CHANGELOG.md`, tests), the `barrel-exports` page, `languages/typescript/` only if code moves there to be shared, `docs/plan.md`, this task file |

## Goal

A consumer asked for one shared rule that says how code imports the files of a kind folder (a
folder that holds only one kind of file) and how it imports the rest of a resource. The
specification is kept outside the repository; the lead gives its path in the brief. It is input, not
a contract: this task file overrides it wherever they differ.

## Decisions (the lead's)

1. **Home.** A new rule, `nestjs/kind-imports`, not aspects of `nestjs/import-aliases`. That rule
   is in both presets with all options defaulted, it checks specifier strings only, and adding
   aspects to it is a breaking change under the plugin's own policy. The new rule is in no preset,
   like the other rules that need a project's options.
2. **Aspects.** `kind-barrel`, `own-kind-folder`, `resource-relative`, `index-segment`,
   `dynamic-import`, `mock-target`. The specification's `structure-drift` is dropped: it checks a
   copy kept for a script outside Gate.
3. **Options, all with neutral defaults:**
   - `kinds`: defaults to `layout.exclusive-kind-folders`; not empty, no duplicates, plain names.
     No consumer's list of kinds is written into Gate.
   - `barrel-name` (`"index.ts"`) and `internal-folder` (`"internal"`), as in `barrel-exports`.
   - `registry` and `registry-import`, defaulting to the values of `nestjs/migrations`. A rule
     cannot read another rule's options.
   - `mock-calls`: `["vi.mock", "vi.doMock", "vi.importActual", "vi.importMock"]`.
4. **Resolution.** Through `layout.path-aliases`, as `import-aliases` does. Resolving tsconfig
   `paths` in this plugin is a plugin-wide decision for later.
5. **Findings.** One finding per load, with its names grouped and joined by `, `, at the load's
   line, keyed by its specifier so that exceptions stay stable. Messages read
   `{path} loads {specifier}; …`, each with a help line.
6. **Reuse.** `typescript/barrel-imports` already builds a barrel's export map that follows
   `export *` through tracked files (`plugins/typescript/src/barrel_imports.rs`). Reuse it rather
   than writing a second one. Code shared by two plugins moves to `languages/typescript`. The rule's
   page says how it relates to `typescript/barrel-imports` and to `nestjs/barrel-exports`.
   `nestjs/barrel-exports` decides kind folders by name. Where its page contradicts the new norm (it
   says an importer names the folder), the page is rewritten in the same change.
7. **Ambiguities in the specification, settled:**
   - `own-kind-folder`, case 2: only for loads whose module is already the target.
   - The wide-mock clause: say "a non-barrel file in no kind folder". A resource barrel does not
     qualify.
   - `/index` segments: checked only for loads that resolve below the source root, as the
     specification's definitions say.
   - `.d.ts` files: they count when a kind folder is decided, and are tried in resolution.
   - The order of the listed kinds in a message: the order of `kinds`.
   - An entity that loads another resource's enum file: the writer adds a fixture and says on the
     page which rule reports it, so that no load is reported twice.
8. **The interface stays `gate:plugin@0.3.0`.** If the work needs a change to it, stop and report
   to the lead before writing code.

## First step: the design, to the lead

Before writing code, send the lead a short design. It covers the load extractor (form, names,
type-only, side-effect, the destructured names of `await import()`, the mocks' first argument), what
is reused from `typescript/barrel-imports`, and where shared code lives.

## Proof

- Every fixture of the specification is rewritten as a synthetic tree (resources such as `book` and
  `author`, kinds from a short synthetic `kinds` list or the default). Each fixture has its pass and
  its expected findings, at file and line.
- No product, domain, folder or norm id of any consumer appears in code, tests, pages, the CHANGELOG
  or commits.
- A speed test on a synthetic tree of about 10 000 files, as in `plugins/nestjs/tests/speed.rs`.
- The comparison with the consumer's reference figures runs outside the repository. The writer
  reports it to the lead and does not commit it.
- All cargo runs go through the lane.

## Resume point

The lead accepted the design, with a seventh option `hoisted-mock-calls` and the skip of an
entity's enum files through `entity-folder` and `enum-kind`. Written: `gate_typescript::loads` and
`gate_typescript::exports`, with `typescript/barrel-imports` moved onto them; the rule under
`plugins/nestjs/src/rules/structure/kind_imports/`, its page, its tests and the CHANGELOG lines.

Both import rules were run with `bin/gate check --format json` on the 28 synthetic trees of the
rule's tests, outside the repository. `typescript/barrel-imports` gives the same findings from the
released `typescript@0.3.1` and from this branch's build on every tree. Next to `kind-imports` it
reports 28 findings with its default options and 0 with `owners = ["src"]` and
`exempt = ["test/**"]`, the configuration the page names; the lead accepted that configuration.
The page also lists the loads inside the roots that only `typescript/barrel-imports` checks with
its default options, each one seen on a synthetic probe tree.

The lane ran the gates on `b3662dea`: fmt, clippy, test (with `--no-fail-fast`), deny and the
release build, each read from `$?` right after its cargo call, all 0. `origin/main` with T-115 was
merged in without a rebase; the conflicts were the guide's table and count, the plan's rows and the
CHANGELOG's Unreleased section.

Next: the gates and the speed measure on the merge, then review.

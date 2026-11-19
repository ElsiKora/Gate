# T-062: One documentation layout for every project, a standard per kind

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead's decision; no code |
| Release | v0.3.0 (it changes the plugin interface, which v0.3.0 moves to `gate:plugin@0.3.0`) |
| Writer | "Gate 005" |
| Reviewer | a separate review session |
| Depends on | the `gate:plugin@0.3.0` bump (T-057 or T-059, whichever merges first); T-061 for the delegated entries |
| Touches | `wit/gate.wit` (`generated-file`, `generate`), `crates/gate-sdk` (a seed and the plugin options in a generator), `crates/gate-host/src/sync` (writing and checking seeds), a new `plugins/docs` (one rule, one generator, the kinds, their pages), `Cargo.toml` (the workspace member), `plugins/agents` (`no-claude-md`: scope and page), `docs/architecture.md` sections 4 and 10, `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

Every project's `docs/` follows the standard of its kind: a service, a frontend, infrastructure, or a
monorepo root. The standards look alike. They have the same top-level names, the same index files,
the same order and the same place for the same kind of content. They differ only where a kind's
nature demands it, and each difference carries its reason. A reader who knows one project finds their
way in any other.

- `gate sync` lays down a project's standard and never overwrites a file a person wrote.
- A project adds an entry of its own only by declaring it, with its job.
- One rule, `docs/layout`, checks that the standard holds.
- The agents and ledger plugins keep what they own inside the layout, and there is no second list of
  their files.

## What was read for this design

Everything was read read-only, by listing tracked files and reading files at `HEAD`. Nothing was run in
any of these repositories, and no name, path or text of theirs is in this one.

- **Four service and monorepo books.**
  - A large service book: about 420 files under 14 top-level entries.
  - Two copies of a smaller service book: 8 entries.
  - The repository-level book of a monorepo: numbered guide chapters with several hundred example data
    files, architecture notes with tab-separated data files, research and release notes. It keeps its
    decisions and follow-ups as two long files inside its architecture directory, and has no index, no
    start page and no decision records.
- **About forty frontend repositories.**
  - Frameworks: React single-page apps built with Vite (about 17), Next.js apps (about 14), one Nuxt
    (Vue) app, and React component libraries and kits (about 6).
  - None has a documentation book. The most any has is three files under `docs/`.
  - Two of them track a `CLAUDE.md` that only imports `AGENTS.md` (see question 6).
- **Two infrastructure projects**, one Terraform, one AWS CDK. Neither has a `docs/`.

So the service books show which entries have a real job, and the root book shows what a monorepo root
keeps. The frontend and infrastructure standards cannot be copied from existing books; they follow
from the same jobs, and the frontend's index sections are drafted below for the owner.

## 1. The layout: one common base, and per-kind differences with their reasons

The layout governs the top-level entries of `docs/` and the sections of each entry's index. What an
entry holds below its index is the entry's own business, so data files, examples and nested
directories are allowed there.

**One name per job.** Each job has one entry, the same in every kind. A NestJS module, a frontend
feature and an infrastructure stack are all "a part of the code with an owner", so all three are
chapters in `modules/`. How-to pages and tutorials are `guide/` in every kind; there is no `how-to/`
anywhere.

### The tree, side by side

A cell is **same** (the entry exists with this job in this kind, as in the others), **same, optional**
(it exists only when the project has that job, the same in every kind), or **absent** (not in this
kind, with the reason). No cell is "kind-specific" at the top level. Where the kinds really differ,
they differ inside an index (the next table), not in the tree. The rows are in the standard order,
which is also the order of the index's entry table.

| # | Entry | Job | Service | Frontend | Infrastructure | Monorepo root | Reason for each cell that is not "same" |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `README.md` | the index: the reading order and a table of every entry | same | same | same | same | |
| 2 | `start-here.md` | the first reading path, for a person or an agent | same | same | same | same | |
| 3 | `architecture/` | how the system is built and why | same | same | same | same | |
| 4 | `modules/` | a chapter per owned part of the code: a module, a feature, a stack | same | same | same | absent | Root: the root owns no code; its parts are member projects, each with its own `docs/`, and the root's index links them (section "Members" below). |
| 5 | `adr/` | decision records: the log and one `NNNN-<slug>.md` per decision | same | same | same | same | |
| 6 | `reference/` | exact values and contracts: formats, inventories, settings, the glossary | same | same | same | same | |
| 7 | `guide/` | task-oriented chapters: how to do one thing, with examples | same | same | same | same | |
| 8 | `testing/` | what the tests are, where they live, what counts as evidence | same | same | same | same | |
| 9 | `operations/` | runbooks: deploy, roll back, recover, rotate | same | same | same | same | |
| 10 | `agents/` | agent rules, skills and the agents' reading map (`agents/context-map.md`); owned by the `agents` plugin | same | same | same | same | |
| 11 | `ledger/` | work records, verification, follow-ups; owned by the `ledger` plugin | same | same | same | absent | Root: work is recorded in the ledger of the member project whose code it changes, and a repository-wide decision is an `adr/` record; a root ledger would be a second place for the same record. |
| 12 | `research/` | exploration that informed a decision but is not one | same, optional | same, optional | same, optional | same, optional | Optional in every kind: present only when the project has done such research. |
| 13 | `releases/` | one page per released version: what a consumer changes | same, optional | same, optional | same, optional | same, optional | Optional in every kind: present only when the project publishes versions. |

Every other top-level name is unknown, unless the project declares it (section 2). One form per
entry: `architecture.md` in place of `architecture/` is a different, unknown entry. Dot entries are
unknown too.

**Reserved names.** These are refused even as a declared extension, because their job has an entry
above. The finding names where the content goes.

| Reserved | Goes to |
| --- | --- |
| `concepts`, `explanation` | `architecture/` |
| `how-to`, `howto`, `tutorials`, `guides` | `guide/` |
| `components`, `features`, `stacks`, `packages` | `modules/` |
| `ai` | `agents/` |
| `decisions` | `adr/` |
| `runbooks` | `operations/` |
| `glossary.md` | `reference/glossary.md` |

**Delegated entries.** `agents/` and `ledger/` are entries of the layout, but their plugins own what
is inside. `docs/layout` never requires them, never lays them down and never reads inside them; their
own rules require their inputs (T-061). In the root kind, a `ledger/` is `unknown`, and the message
gives the reason from the table.

### The index sections, side by side

Each index of a laid-down entry has required level-2 sections, in this order. The rule checks them,
and the seed writes them as headings. A cell is **same** (the same sections in every kind), or
**kind-specific** (the sections this kind's nature needs), with the reason.

| Index | Service | Frontend (draft, open for the owner) | Infrastructure | Monorepo root | Reason for the kind-specific cells |
| --- | --- | --- | --- | --- | --- |
| `README.md` | same: Reading order, Entries | same | same | Reading order, Entries, **Members** | Root: its parts are member projects, and a reader needs a table that links each member's `docs/README.md`. |
| `start-here.md` | same: Mental model, First day | same | same | same | |
| `architecture/README.md` | kind-specific: System shape, API surface, Data, Runtime | kind-specific: System shape, Routing, State, Rendering, API access | kind-specific: System shape, Environments, Networking, Secrets and parameters | kind-specific: System shape, Members and dependencies, Build and release flow | What "how the system is built" covers is the kind's nature: a service has an API and data; a frontend has routes, state and a rendering model; infrastructure has environments and a network; a root has members and a build order. "System shape" opens every one. |
| `modules/README.md` | same: Chapter map, Adding a module | same | same | (absent) | |
| `adr/README.md` | same: Log, Template | same | same | same | |
| `reference/README.md` | same: Contents | same | same | same | |
| `guide/README.md` | same: Pages | same | same | same | |
| `testing/README.md` | same: Kinds of test, Evidence | same | same | same | |
| `operations/README.md` | same: Runbooks | same | same | same | |

**The frontend row is the open item.** No frontend of ours has a documentation book, so its
architecture sections are drafted from the job, not read from a tree. One frontend kind covers React
with Vite, Next.js and Nuxt (Vue), the frameworks in use.

There is no pack per framework, by the owner's own rule: their trees and indexes would be identical,
and an entry without a reason for a difference goes into the base. "Rendering" holds the framework's
real difference (single-page, server-rendered, or both) as content, not as a different tree. If the
owner finds a section a framework needs and the others do not, that is a reason, and the kind splits
then.

**Not covered: a library.** A package published for others to use is not one of the four kinds
(component libraries, SDKs, a monorepo's `packages/*`). It is an open item for the owner; the same
table would take a fifth column.

### The four books mapped

| Book | Stays | Moves | Goes | Is added (laid down, then filled) |
| --- | --- | --- | --- | --- |
| Large service book (service) | index, start page, `architecture/`, `modules/`, `adr/`, `reference/`, `testing/`, `operations/`, `agents/`, `ledger/` | the glossary into `reference/`; the "AI" reading map into `agents/context-map.md`; the "how-to" link table becomes `guide/README.md` | the "concepts" link table, folded into the index | nothing |
| Smaller service books (service) | index, start page, `modules/`, `adr/`, `reference/`, `agents/`, `ledger/` | the "AI" reading map into `agents/context-map.md` | nothing | `architecture/`, `guide/`, `testing/`, `operations/` |
| Repository-level book (monorepo root) | `architecture/`, `guide/`, `research/`, `releases/` | the decisions file into `adr/` records; the follow-ups file into the ledgers of the members they concern | nothing | `README.md` (with Members), `start-here.md`, `adr/`, `reference/`, `testing/`, `operations/` |
| Frontends and infrastructure | | the few existing pages into `guide/` | | the whole standard of their kind |

The moves are each project's own work after this task. This task only makes them checkable.

## 2. Where the standards live, and how a project picks its kind

**Recommendation: the four kinds are named standards inside the `docs` plugin.** The kinds are
`service`, `frontend`, `infrastructure` and `monorepo-root`. Each is the common base plus its
differences from the tables above, as constants of the plugin. The core names no kind and no framework;
the plugin names kinds, not frameworks.

The alternative was a layout fragment in each stack's plugin (`nestjs`, `typescript`, a future frontend
plugin) that `docs/layout` composes. It is not recommended:

- The standards must look alike. Spread across plugins with their own releases, they drift apart,
  which is the failure the owner names. In one plugin, the side-by-side table above is one source file,
  and a difference without a reason is visible in review.
- Two of the four kinds, infrastructure and the monorepo root, have no stack plugin to hold their
  fragment.
- Plugins share nothing at run time. Composing fragments from other plugins would need a new core
  mechanism, to pass one plugin's declarations into another plugin's rule, for data that is a table.
- Since the standards differ by kind and not by framework, a stack plugin would have nothing of its own
  to contribute.

**A project picks its kind explicitly, in one line,** in the plugin's shared options. The rule and the
generator both read them:

```toml
[plugins]
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "service" } }
```

- `kind` is required. A missing or unknown kind is a configuration error (exit 2) that lists the four.
  It is never inferred from the stack plugins a project enables: a plugin cannot see which others are
  enabled, an infrastructure or root project may enable none, and a project that enables `typescript`
  could be any of the four.
- In a monorepo, each project sets its own kind in its own `gate.toml`. The root project sets
  `monorepo-root`; a profile can set it for a set of projects.

**A project's extensions** are the `extra` key of the same options:

```toml
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "service", extra = [{ name = "playbooks", job = "Step-by-step plays for incidents nobody has automated yet" }] } }
```

The rules for an extension:
- `name` is kebab-case: a directory (with `README.md`) or a Markdown file (`name` ending in `.md`).
- It is not an entry of the standard (for this kind or any other), a delegated entry or a reserved name.
- `job` is a sentence saying what the entry holds.

Breaking any of these is a configuration error at load, naming the key. A declared entry that does not
exist is a finding (`#extra-missing`). Extensions come after the standard's entries in the index's
order, in the order declared.

**Why no manifest file?** The ledger has a manifest because a real ledger is mostly the project's own
files (T-061, question 1). The documentation standard is the other way round: almost all of it is fixed,
and a project adds a line or two. Options are validated against a schema and layered with the rest of
the configuration.

**No second list.** The standards name `agents/` and `ledger/` and nothing inside them. The ledger
manifest stays the one list of ledger files, and the agents plugin's rules stay the one source for
agent files. A missing delegated entry is a finding of its owner's rules only, so one missing file
never gets findings from both plugins.

## 3. The command that lays it down

`gate sync`, through a generator of the `docs` plugin, `docs/layout`. There is no new command.

Today a generator's file is entirely its own: `gate sync` writes it whenever its contents differ, and
`gate sync --check` reports any difference as drift. That cannot lay down a page a person then fills,
because the next `gate sync` would put the empty page back. So the interface gets two changes, both on
`gate:plugin@0.3.0`:

```wit
/// A file a generator writes, at a path relative to the project root; the host maps it. A seed is
/// written only when nothing exists at its path; after that it belongs to the project.
record generated-file { path: string, contents: list<u8>, seed: bool }

/// `plugin-options`: JSON, the `[plugins.<name>]` options, as a check call receives them.
export generate: func(name: string, project: string, options: string, plugin-options: string)
    -> result<generate-result, plugin-error>;
```

The second change lets the generator read `kind` from the same line as the rule. Without it, the kind
would have to be written twice, once in `[generate]` and once in `[rules]`.

| | An owned file (`seed: false`, today) | A seed (`seed: true`) |
| --- | --- | --- |
| `gate sync` | writes it when the contents differ | writes it only when nothing exists at the path in the working tree (no file, link or directory, tracked or not); otherwise leaves it alone |
| `gate sync --check` | reports it missing or different (`gate/sync#<generator>`) | never reports it: a seed claims nothing about contents, and whether the file must exist is a rule's question |
| owned globs | a tracked file under them that no generator writes is stale | a seed under a glob its generator owns is refused: the call fails as an environment error, because a seed belongs to the project from the moment it is written |
| reported after `gate sync` | among the written files, and the untracked ones to `git add` | the same |

A seed's contents are never compared, so a person's edit is never overwritten. Because the test is
whether the path exists, an untracked file someone just wrote is not overwritten either. A seed is
written again only after its file is deleted. The generator seeds exactly the indexes the kind
requires, so a required file deleted on purpose comes back, which is what "required" means.

**What it writes on an empty project.** With `docs/layout` in `[generate]` and no `docs/`, `gate sync`
writes the required index of each non-optional, non-delegated entry of the kind:

| | Service | Frontend | Infrastructure | Monorepo root |
| --- | --- | --- | --- | --- |
| files | 9 | 9 | 9 | 8 |

- The nine are `README.md`, `start-here.md`, and the `README.md` of `architecture/`, `modules/`,
  `adr/`, `reference/`, `guide/`, `testing/` and `operations/`.
- The root has no `modules/`. Its `README.md` has the "Members" section.
- Each file is a heading, the kind's required sections as level-2 headings in order, one line under
  each saying what goes there, and the marker line `<!-- gate:seed: replace this page's text, then delete this line -->`.
- The index's entry table links every seeded entry in the standard order.
- `adr/README.md` carries the record template as a fenced block. There is no template file, which
  would come back each time someone deleted it.
- The delegated entries are laid down by their owners, if at all (see "Open").

**What it writes on an existing project.** Only the required paths where nothing exists. On the smaller
service books, that is the indexes of `architecture/`, `guide/`, `testing/` and `operations/`. It never
deletes, and it never touches an optional, delegated or extension entry, or a file that is already
there.

**How `gate sync --check` treats it.** It does not report seeds. A missing required entry is a finding
of `docs/layout` in `gate check`, once, and `gate sync` is its fix. This is the T-061 principle: one
finding per missing input, from the rule that reads it.

The generator reads no tree; it decides nothing from the tree, since the core decides whether to write.
Its only input is `kind`.

## 4. The rule that checks it

`docs/layout`, project scope. It reads the tracked paths under `docs/` and the text of the required
indexes. It does not read history.

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `missing` | a required entry of the kind is not tracked (for a directory entry, its `README.md`) | the entry's path | the entry |
| `unknown` | a top-level entry is not in the kind's standard and not declared. For a reserved name, or an entry the kind marks absent, the message says where the content goes, or why the kind has none. | the entry's path | the entry |
| `extra-missing` | a declared extension is not tracked | `docs/` | the declared name |
| `entry-index` | a present directory entry that is optional or declared has no `README.md`. Delegated entries are their owners' business. | the directory | the entry |
| `index` | `docs/README.md`'s entry table has no local link to a tracked top-level entry, or lists the entries out of the standard order | `docs/README.md` | the entry |
| `sections` | an index lacks a required section of its kind, or has them out of order. Other sections may follow the required ones. | the index | the index and the section |
| `unfilled` | a tracked file of a required entry still holds the seed marker line | the marker's line | the file |

Example findings:

```text
error docs/layout#unknown docs/ledger
  docs/ledger is not an entry of a monorepo-root: work is recorded in the ledger of the member project whose code it changes
error docs/layout#missing docs/start-here.md
  docs/start-here.md is a required entry of a service; run gate sync to lay it down, then fill it
error docs/layout#sections docs/architecture/README.md
  docs/architecture/README.md has no section "Routing", which a frontend's architecture index has after "System shape"
```

The level is the configuration's, as always. A project that is migrating can set
`levels = { unfilled = "warning" }`, or use exceptions, like any other aspect. The rule has no options
of its own; it reads `kind` and `extra` from the plugin's.

## 5. Which plugin owns it

A new plugin, `docs`, with the rule `docs/layout`, the generator `docs/layout` and the four kinds. The
others do not fit:

- **`agents`** owns one entry (`agents/`) and the files Claude Code reads. If it owned the layout, a
  project without coding agents could not have a checked layout.
- **`ledger`** owns one entry (`ledger/`), and the root kind has no ledger at all.
- **`markdown`** checks what is inside Markdown files anywhere in a project. The layout is the shape of
  one directory, with a generator and kinds. Folding it into `markdown` would make a project that only
  wants its Markdown checked take on the layout too.
- **A stack plugin** (`nestjs`, `typescript`) holds no kind of its own (section 2), and three of the
  kinds have no stack plugin.

A separate plugin keeps each plugin to one subject. A project turns the layout on or off on its own.
The cost is one more plugin to build, pin and release. The plugin release workflow already takes any
`plugins/<name>` by its tag.

## 6. `CLAUDE.md`

**Decision: a `CLAUDE.md` stays forbidden, at every level and in every kind, with no exception.
`agents/no-claude-md` moves to repository scope, so a root file is always seen.**

Claude Code's documentation (code.claude.com/docs/en/memory, read 2026-11-19) settles the question:

- Claude Code reads `AGENTS.md` itself as the project's instructions, with no `CLAUDE.md`, import or
  setting. A nested `AGENTS.md` loads when Claude opens a file in that directory.
- It does this "only when you have no `CLAUDE.md` in your working directory or above it". When both
  exist, it reads the `CLAUDE.md` files only.

So a `CLAUDE.md` that is exactly `@AGENTS.md` does not make Claude Code read `AGENTS.md`, because it
already does that. The file switches Claude Code into its `CLAUDE.md`-only mode for every directory
below it. In a monorepo, a root import file hides every nested `AGENTS.md` that has no import file of
its own. The rule's message, "a tracked CLAUDE.md disables AGENTS.md", is literally what the
documentation describes. Single repositories with such an import file exist among those read for this
design. There the file is redundant rather than harmful, and the rule makes them delete it.

The alternatives, and why not:

- **Allowing a file that is exactly `@AGENTS.md`** is an exception by its contents, and it still hides
  every nested `AGENTS.md`.
- **Requiring one, generated next to every `AGENTS.md`** (the `agents/adapters` generator writing it and
  owning `**/CLAUDE.md`) would be one clean rule. But it makes every repository carry a file that
  current Claude Code does not need, only for clients older than the version that reads `AGENTS.md`
  itself. A repository should not encode its readers' client versions.

The hole is real. A monorepo can have a root `CLAUDE.md` that only imports `AGENTS.md`, with the rule
enabled only for its nested projects. A project-scope rule cannot see the root file, and the root file
is exactly what hides the nested `AGENTS.md`. So:

- `agents/no-claude-md` becomes a repository-scope rule. It runs once, sees every tracked path, and
  reports every `CLAUDE.md` in any case.
- A repository-scope rule is set in the root `gate.toml`. The core already gives a configuration error
  for one set below the root. The rule's page and `docs/guide/plugins.md` say so.
- The `.claude/rules` and `.claude/skills` adapters stay as they are. Claude Code reads them whether or
  not a `CLAUDE.md` exists, and they carry the rule files, not the entry.
- `CHANGELOG.md` (0.3.0, breaking): `agents/no-claude-md` moves from the projects' files to the root one.

## 7. Tests

Native SDK tests per aspect and per kind, core tests for the seed, and one end-to-end run through the
built components. Each case is one that would let a broken change through, or block a good one.

**The standards (a unit test of the plugin's constants).** These are what keep the kinds alike:

- every kind has the same entries in the same order, except the cells the side-by-side table marks
  absent, which are exactly `modules/` and `ledger/` in the root kind;
- every non-"same" cell has a reason string;
- no entry name of one kind is reserved;
- the index sections are equal across kinds except in the cells marked kind-specific.

A change that adds a difference without a reason fails here.

**Seeds (core, `crates/gate-host/src/sync`):**

- A seed whose path is absent is written, and it is listed as written and untracked.
- A seed whose path holds a tracked file with other text is not written, and neither is one whose path
  holds an untracked file, a directory or a symbolic link.
- `gate sync --check` reports nothing for a seed, whether it is absent or edited. An owned file beside it
  still reports drift as today.
- A seed under its generator's owned glob fails the call as an environment error naming the path.
- A seed is never stale: removing the generator from `[generate]` removes no seeded file.
- A generator receives `plugin-options`.

**Generator `docs/layout`, per kind:**

- An empty project gets exactly the kind's files (9, 9, 9, 8), each with the marker line, the kind's
  sections in order, and the index's entry table in the standard order.
- A complete project gets nothing written.
- A service missing only `docs/testing/README.md` gets that one file.

**Rule `docs/layout`, one fixture per aspect, each with a passing twin:**

- `missing`:
  - each required entry of each kind absent, one at a time, gives one finding;
  - `docs/architecture/` holding files but no `README.md` is `missing`, not `entry-index`.
- `unknown`:
  - `docs/concepts/` names `docs/architecture`, `docs/ai/` names `docs/agents`, `docs/how-to/` names
    `docs/guide`, and `docs/glossary.md` names `docs/reference/glossary.md`;
  - `docs/ledger/` and `docs/modules/` in a monorepo root each give the table's reason;
  - `docs/notes/`, `docs/architecture.md` and `docs/.gitkeep` are each `unknown`;
  - a nested path (`docs/guide/examples/x.json`, `docs/architecture/map.tsv`) never is.
- `kind`: missing, or `"backend"`, exits 2 listing the four.
- `extra`:
  - a declared `playbooks/` with `README.md` passes, and comes after the standard entries in the
    index's order;
  - declared but absent gives `extra-missing`;
  - a declared `concepts`, `ledger` (in any kind), `modules` or `Playbooks` exits 2, naming the key.
- `entry-index`: `docs/research/` with pages and no `README.md` is reported. `docs/ledger/` and
  `docs/agents/` without one are not.
- `index`:
  - an index linking every present entry in order passes;
  - a present `docs/research/` that is not linked is reported;
  - `guide/` listed before `adr/` is reported as out of order.
- `sections`:
  - a frontend architecture index without "Routing" is reported;
  - a service's with the same sections in another order is reported;
  - extra sections after the required ones pass;
  - the root `README.md` without "Members" is reported.
- `unfilled`: the seeded pages give one finding each, at the marker line, and deleting the line
  clears it.
- **Delegated entries:** with `docs/ledger/` and `docs/agents/` absent in a service, `docs/layout`
  reports nothing about them. The T-061 `#input` findings remain the only ones.

**`agents/no-claude-md`, repository scope:**

- A root `CLAUDE.md` holding only `@AGENTS.md` is reported, and so is a nested `x/CLAUDE.md` and a
  `claude.md` in lower case.
- A repository with neither passes.
- Setting the rule in a nested project's `gate.toml` is a configuration error.

**End to end, once per kind:** an empty repository with the `docs` plugin pinned and the kind set:

- `gate check` exits 1 with one `missing` per required entry;
- `gate sync` writes the kind's files;
- `gate check` exits 1 with one `unfilled` per seeded page;
- with the marker lines removed, `gate check` and `gate sync --check` exit 0.

**Keys:** every finding is keyed by its entry, file or section, so editing text around it never changes
its fingerprint.

## Open, for the owner

1. **The frontend index sections** (the draft row above). No frontend of ours has a book to read them
   from.
2. **A fifth kind for libraries and packages**: component libraries, SDKs, and a monorepo's
   `packages/*`.
3. **Whether `agents` and `ledger` seed their own minimum** with the same seed mechanism, after T-061:
   an `AGENTS.md` with the block markers, a ledger manifest with the plugin's default files. Then one
   `gate sync` lays down the whole standard, delegated entries included. It is left out here, to keep
   each plugin's change in its own task.

## Done when

- The lead decides on questions 1 to 6 and on the open items. Then the code goes on the tip that carries
  `gate:plugin@0.3.0`, followed by one lane round on the pushed tip: fmt, clippy, test
  (`--workspace --no-fail-fast`), deny and the release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Resume point

2026-11-19: design written from `main` at `ec66737`, then reworked for the owner's two additions (a
standard per kind; the kinds look alike, each difference with its reason). Waiting for the lead's
decision before any code.

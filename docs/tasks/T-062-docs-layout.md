# T-062: One documentation layout for every project, a standard per kind

| Field | Value |
| --- | --- |
| Status | done (merged at `7b525d6`) |
| Release | v0.3.0 (it changes the plugin interface, which v0.3.0 moves to `gate:plugin@0.3.0`) |
| Writer | "Gate 005" |
| Reviewer | a separate review session |
| Depends on | nothing: this branch carries the `gate:plugin@0.3.0` bump, and whichever of T-057, T-059 and T-062 merges later rebases onto it. T-063 builds on it for the delegated entries |
| Touches | `wit/gate.wit` (`generated-file`, `generate`), `crates/gate-sdk` (a seed and the plugin options in a generator), `crates/gate-host/src/sync` (writing and checking seeds), a new `plugins/docs` (one rule, one generator, the kinds, their pages), `Cargo.toml` (the workspace member), `plugins/agents` (`no-claude-md`: scope and page), `docs/architecture.md` sections 4 and 10, `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

Every project's `docs/` follows the standard of its kind: a service, a frontend, infrastructure, a
library, or a monorepo root. The standards look alike. They have the same top-level names, the same index files,
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
from the same jobs. The frontend's index sections were drafted from the job and taken as they are (the
owner may adjust them later); the library kind was added by the lead for the React libraries and Node
SDKs among those read.

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
(it exists only when the project has that job), **required** (optional in the other kinds, required
in this one, with the reason), or **absent** (not in this kind, with the reason). No cell is "kind-specific" at the top level. Where the kinds really differ,
they differ inside an index (the next table), not in the tree. The rows are in the standard order,
which is also the order of the index's entry table.

| # | Entry | Job | Service | Frontend | Infrastructure | Library | Monorepo root | Reason for each cell that is not "same" |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `README.md` | the index: the reading order and a table of every entry | same | same | same | same | same | |
| 2 | `start-here.md` | the first reading path, for a person or an agent | same | same | same | same | same | |
| 3 | `architecture/` | how the system is built and why | same | same | same | same | same | |
| 4 | `modules/` | a chapter per owned part of the code: a module, a feature, a stack, a package | same | same | same | same | absent | Root: the root owns no code; its parts are member projects, each with its own `docs/`, and the root's index links them (section "Members" below). |
| 5 | `adr/` | decision records: the log and one `NNNN-<slug>.md` per decision | same | same | same | same | same | |
| 6 | `reference/` | exact values and contracts: formats, inventories, settings, the glossary | same | same | same | same | same | |
| 7 | `guide/` | task-oriented chapters: how to do one thing, with examples | same | same | same | same | same | |
| 8 | `testing/` | what the tests are, where they live, what counts as evidence | same | same | same | same | same | |
| 9 | `operations/` | runbooks: deploy, roll back, recover, rotate, release | same | same | same | same | same | |
| 10 | `agents/` | agent rules, skills and the agents' reading map (`agents/context-map.md`); owned by the `agents` plugin | same | same | same | same | same | |
| 11 | `ledger/` | work records, verification, follow-ups; owned by the `ledger` plugin | same | same | same | same | absent | Root: work is recorded in the ledger of the member project whose code it changes, and a repository-wide decision is an `adr/` record; a root ledger would be a second place for the same record. |
| 12 | `research/` | exploration that informed a decision but is not one | same, optional | same, optional | same, optional | same, optional | same, optional | Optional in every kind: present only when the project has done such research. |
| 13 | `releases/` | one page per released version: what a consumer changes | same, optional | same, optional | same, optional | required | same, optional | Optional where the project publishes versions. Library: required, because publishing versions is a library's nature, and the consumer of each version needs the page that says what to change. Its index has one section, Versions. |

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

**The monorepo root, checked against the rules that apply there.** Is a ledger required at the root,
or delegated to the projects? Delegated. This follows from how the core assigns paths:

- A path belongs to the project with the longest root that contains it. In a workspace that does not
  list `"."`, a root `docs/` belongs to no project, and only repository-scope rules see it. Every
  ledger rule is project scope, and so is `docs/layout`. So today no ledger rule can require a ledger
  at such a root, and no layout rule can check its `docs/` either.
- For the root's `docs/` to be checked, the workspace lists `"."` as a project, with
  `kind = "monorepo-root"`. Its subtree is every path no longer-rooted project owns: the root's
  `docs/`, `README.md` and `AGENTS.md`.
- That root project enables no ledger rule, whether in its own file or through a profile that matches
  it. Each member project that changes code keeps its own ledger (`ledger/` is "same" for a service,
  a frontend and infrastructure). A project that shares another's ledger reads it through a `reads`
  grant, as T-061 already allows.
- If the root project does enable a ledger rule, the two plugins disagree in plain sight: T-061
  reports the missing ledger, and once one is added, `docs/layout#unknown` reports it with the reason.
  Neither plugin can see the other's settings, so the fix is the root's configuration. The rule's page
  says so.

### The index sections, side by side

Each index of a laid-down entry has required level-2 sections, in this order. The rule checks them,
and the seed writes them as headings. A cell is **same** (the same sections in every kind), or
**kind-specific** (the sections this kind's nature needs), with the reason.

| Index | Service | Frontend | Infrastructure | Library | Monorepo root | Reason for the kind-specific cells |
| --- | --- | --- | --- | --- | --- | --- |
| `README.md` | same: Reading order, Entries | same | same | same | Reading order, Entries, **Members** | Root: its parts are member projects, and a reader needs a table that links each member's `docs/README.md`. |
| `start-here.md` | same: Mental model, First day | same | same | same | same | |
| `architecture/README.md` | kind-specific: System shape, API surface, Data, Runtime | kind-specific: System shape, Routing, State, Rendering, API access | kind-specific: System shape, Environments, Networking, Secrets and parameters | kind-specific: System shape, Public API, Versioning and release, Compatibility | kind-specific: System shape, Members and dependencies, Build and release flow | What "how the system is built" covers is the kind's nature: a service has an API and data; a frontend has routes, state and a rendering model; infrastructure has environments and a network; a library has a public API that others build on, a release line and the versions it supports; a root has members and a build order. "System shape" opens every one. |
| `modules/README.md` | same: Chapter map, Adding a module | same | same | same | (absent) | |
| `adr/README.md` | same: Log, Template | same | same | same | same | |
| `reference/README.md` | same: Contents | same | same | same | same | |
| `guide/README.md` | same: Pages | same | same | same | same | |
| `testing/README.md` | same: Kinds of test, Evidence | same | same | same | same | |
| `operations/README.md` | same: Runbooks | same | same | same | same | |

**The frontend row** is drafted from the job, not read from a tree, since no frontend of ours has a
documentation book; the lead took it as it is (2026-11-19), and the owner may adjust it later. One
frontend kind covers React with Vite, Next.js and Nuxt (Vue), the frameworks in use.

There is no pack per framework, by the owner's own rule: their trees and indexes would be identical,
and an entry without a reason for a difference goes into the base. "Rendering" holds the framework's
real difference (single-page, server-rendered, or both) as content, not as a different tree. If the
owner finds a section a framework needs and the others do not, that is a reason, and the kind splits
then.

**The library kind** (decided by the lead, 2026-11-19) is a package published for others to use:
component libraries, SDKs, a monorepo's `packages/*`. It has the same tree as a service. Only its
architecture index differs, with the reason in the table. "System shape" opens it, as it opens every
kind's.

### The four books mapped

| Book | Stays | Moves | Goes | Is added (laid down, then filled) |
| --- | --- | --- | --- | --- |
| Large service book (service) | index, start page, `architecture/`, `modules/`, `adr/`, `reference/`, `testing/`, `operations/`, `agents/`, `ledger/` | the glossary into `reference/`; the "AI" reading map into `agents/context-map.md`; the "how-to" link table becomes `guide/README.md` | the "concepts" link table, folded into the index | nothing |
| Smaller service books (service) | index, start page, `modules/`, `adr/`, `reference/`, `agents/`, `ledger/` | the "AI" reading map into `agents/context-map.md` | nothing | `architecture/`, `guide/`, `testing/`, `operations/` |
| Repository-level book (monorepo root) | `architecture/`, `guide/`, `research/`, `releases/` | the decisions file into `adr/` records; the follow-ups file into the ledgers of the members they concern | nothing | `README.md` (with Members), `start-here.md`, `adr/`, `reference/`, `testing/`, `operations/` |
| Frontends and infrastructure | | the few existing pages into `guide/` | | the whole standard of their kind |

The moves are each project's own work after this task. This task only makes them checkable.

## 2. Where the standards live, and how a project picks its kind

**Decision: the five kinds are named standards inside the `docs` plugin.** The kinds are
`service` (the NestJS services today; the kind is named for the job, not the framework), `frontend`,
`infrastructure`, `library` and `monorepo-root`. Each is the common base plus its
differences from the tables above, as constants of the plugin. The core names no kind and no framework;
the plugin names kinds, not frameworks.

The alternative was a layout fragment in each stack's plugin (`nestjs`, `typescript`, a future frontend
plugin) that `docs/layout` composes. It is not recommended:

- The standards must look alike. Spread across plugins with their own releases, they drift apart,
  which is the failure the owner names. In one plugin, the side-by-side table above is one source file,
  and a difference without a reason is visible in review.
- Three of the five kinds, infrastructure, library and the monorepo root, have no stack plugin to hold their
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

- `kind` is required. A missing or unknown kind is a configuration error (exit 2) that lists the five.
  It is never inferred from the stack plugins a project enables: a plugin cannot see which others are
  enabled, an infrastructure or root project may enable none, and a project that enables `typescript`
  could be any of the five.
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

| | Service | Frontend | Infrastructure | Library | Monorepo root |
| --- | --- | --- | --- | --- | --- |
| files | 9 | 9 | 9 | 9 | 8 |

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

A new plugin, `docs`, with the rule `docs/layout`, the generator `docs/layout` and the five kinds. The
others do not fit:

- **`agents`** owns one entry (`agents/`) and the files Claude Code reads. If it owned the layout, a
  project without coding agents could not have a checked layout.
- **`ledger`** owns one entry (`ledger/`), and the root kind has no ledger at all.
- **`markdown`** checks what is inside Markdown files anywhere in a project. The layout is the shape of
  one directory, with a generator and kinds. Folding it into `markdown` would make a project that only
  wants its Markdown checked take on the layout too.
- **A stack plugin** (`nestjs`, `typescript`) holds no kind of its own (section 2), and three of the
  five kinds have no stack plugin.

A separate plugin keeps each plugin to one subject. A project turns the layout on or off on its own.
The cost is one more plugin to build, pin and release. The plugin release workflow already takes any
`plugins/<name>` by its tag.

## 6. `CLAUDE.md`

**Decision: a `CLAUDE.md` stays forbidden, at every level and in every kind, with no exception.
`agents/no-claude-md` moves to repository scope, so that a root file is always seen. This is a
breaking change in 0.3.0.**

Claude Code's memory page (code.claude.com/docs/en/memory, read 2026-11-19) says two things that
settle it:

- Claude Code reads `AGENTS.md` itself, as the project's instructions, when there is no `CLAUDE.md`
  or `CLAUDE.local.md` in the working directory or above it.
- When both are present, it reads only the `CLAUDE.md` files by default.

So:

- **A `CLAUDE.md` that only imports `AGENTS.md`** adds nothing for current Claude Code, which reads
  `AGENTS.md` without it.
- **A `CLAUDE.md` with any more than that** becomes the instructions Claude Code reads in place of
  `AGENTS.md`. Other agents never read that second set, and it drifts from the first. That drift is
  what `agents/no-claude-md` exists to prevent.

The alternatives, and why not:

- **Allowing a file that is exactly `@AGENTS.md`** is an exception by its contents, and it buys nothing
  for a client that reads `AGENTS.md` itself.
- **Requiring one, generated beside every `AGENTS.md`** (the `agents/adapters` generator writing and
  owning it) would be one clean rule. But it makes every repository carry a file that only older clients
  need. A repository should not encode the versions of the clients that read it.

**Why repository scope.** Some repositories set the rule only for their nested projects. The root
`CLAUDE.md` belongs to no such project, so a project-scope rule never sees it, and the page reads
`CLAUDE.md` from the working directory or above it. So:

- `agents/no-claude-md` becomes a repository-scope rule. It runs once, sees every tracked path, and
  reports every `CLAUDE.md` in any case.
- A repository-scope rule is set in the root `gate.toml`. The core already gives a configuration error
  for one set below the root. The rule's page and `docs/guide/plugins.md` say so.
- The `.claude/rules` and `.claude/skills` adapters stay as they are. They carry the rule files, not the
  entry.
- `CHANGELOG.md` (0.3.0, Breaking Changes): `agents/no-claude-md` moves from the projects' files to the
  root one. Delete any `CLAUDE.md` that only imports `AGENTS.md`, and move the instructions of any other
  `CLAUDE.md` into `AGENTS.md` or a rule file.

Repositories with an import-only `CLAUDE.md` exist among those read for this design. Under this rule
they delete it, and Claude Code goes on reading their `AGENTS.md`.

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

- An empty project gets exactly the kind's files (9, 9, 9, 9, 8), each with the marker line, the kind's
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
- `kind`: missing, or `"backend"`, exits 2 listing the five.
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

**The monorepo root as a project:** a workspace that lists `"."` and `apps/a` with `kind =
"monorepo-root"` for `"."`:

- `docs/layout` for `"."` sees the root's `docs/` and not `apps/a/docs/`;
- a root `docs/ledger/` is `unknown`, with the reason;
- `apps/a` checks its own kind independently.

**Keys:** every finding is keyed by its entry, file or section, so editing text around it never changes
its fingerprint.

## Decided (the lead, 2026-11-19)

1. **The frontend index sections**: the draft row is taken as it is: Routing, State, Rendering, API
   access, after System shape. The owner may adjust them later.
2. **A fifth kind, `library`**, for packages and SDKs: Public API, Versioning and release,
   Compatibility, after System shape. Same tree; every difference has its reason in the table.
3. **The agents and ledger seeds**: follow-up T-063, below.

## Follow-up: T-063

Decided by the lead (2026-11-19): after T-061, the `ledger` and `agents` plugins seed their own minimum
through the same seed mechanism.

- `ledger`: the manifest and the files it lists.
- `agents`: `AGENTS.md` with its managed block markers, and `docs/agents/rules/`.

One `gate sync` then lays down a project's whole standard, the delegated entries included. That work
is T-063. It is not part of T-062's code.

## Done when

- The code on a branch from `main`, its WIT lines (seed, `plugin-options`) on `gate:plugin@0.3.0`;
  whichever of T-057, T-059 and T-062 merges later rebases them. Then one lane round on the pushed tip: fmt, clippy, test
  (`--workspace --no-fail-fast`), deny and the release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Presets (the lead, 2026-11-20)

Every plugin new in 0.3.0 ships `<plugin>/recommended`, every rule at error, and `<plugin>/adoption`,
every rule at warning, built like `git/recommended`: here `docs/recommended` and `docs/adoption`. A
native test checks that each enables exactly the plugin's rules at its level, and a CLI test that
each turns `docs/layout` on at that level with no `[rules]` line.

## Delegated entries (the lead, 2026-11-20)

The bar: after `gate sync` on an empty project, `gate check` reports no structural finding, only
content findings (`#unfilled`, and the input findings of the delegated plugins' own rules).

- `delegated` in the `[plugins.docs]` options lists the delegated entries whose plugins the project
  enables (`agents`, `ledger`); empty by default. A name that is not a delegated entry, an entry the
  kind does not have, or a name listed twice is a configuration error naming the key.
- The seed of `docs/README.md` has a row for each listed entry, linking its directory.
- `#index` requires a row for each listed entry and for any delegated directory that exists.
- `#delegated-missing`: a listed entry with no tracked file under it; the finding says to run
  `gate sync` or remove it from `delegated`.
- A preset carries rule options only, not plugin options, so `delegated` is documented on the
  `docs/layout` pages and set by the project.
- Tests: native (the seed's rows, the index rule, the missing entry, the configuration errors) and
  one CLI test per kind with `docs/agents/` and `docs/ledger/` tracked as stand-ins: after
  `gate sync`, every `docs/layout` finding is `#unfilled`; with the entry removed, one
  `#delegated-missing`. The three-plugin test is T-063's.

## Review fix round (the lead, 2026-11-20)

The review of `ec96a38`, fixed in one round, with the `delegated` option above:

1. Rebased onto `main` at `e9e1434` (T-056's cache: `results::load(cache, plugin, key)`, format
   3, `results/<plugin sha256>/`); the generator's cache calls use that form.
2. A seed under a glob any generator owns is refused in `plan`, as an environment error naming both
   generators; before, only its own generator's globs were checked, so a page seeded under another
   generator's glob was deleted as stale once its own generator stopped running.
3. A CLI test that the generator's stored output is keyed by the plugin's options.
4. The seed host test seeds outside `generated/**` and removes the generator; a CLI test that a
   direct commit editing a seeded page is a code change for `git/main-history`, not a generated
   file in sync.
5. A missing required plugin option names where to set it and, when its schema lists them, the
   values it takes.
6. An extension reuses no standard entry's name or stem, of any kind, in any case.
7. A library requires `releases/`, with an index of one section, Versions. `operations/` stays
   required in a library and a monorepo root; the jobs of `modules/` (a package) and
   `operations/` (release) are the code's, and the table above says so.
8. The CHANGELOG gives the whole `agents/no-claude-md` migration.
9. Status, the dependency line, and the plan's T-062 and T-063 rows.
10. Spec tests: each required entry of each kind absent alone; a nested project setting
    `agents/no-claude-md` at the CLI; the root listed as `"."`.

## Proof

Lane round on `6f4019d` (ticket 268, 2026-11-20, 00:41 to 00:49 UTC, load average about 69 at the
start), in the target directory T-046 left:

| Step | Command | Exit |
| --- | --- | --- |
| fmt | `cargo fmt --all --check` | 0 |
| clippy | `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 |
| test | `cargo test --locked --workspace --no-fail-fast` | 0 (1062 passed, 0 failed) |
| deny | `cargo deny --locked check` | 0 |
| build | `cargo build --release --locked` | 0 |

Ticket 266 on the same tip was withdrawn before it started, so the T-047 measures (ticket 267)
ran first.

Lane round on `92b2b6f`, the presets (ticket 278, 2026-11-20, 01:56 to 02:00 UTC, load average about
37 at the start), same target directory: fmt 0, clippy 0, test 0 (1064 passed, 0 failed, the two
preset tests among them), deny 0, build 0.

Lane rounds of the review's fix round, after the rebase onto `e9e1434`, same target directory:

- ticket 286 on `fa41f8e`: fmt 0, clippy 101 (E0382 in the new delegated native test: `without`
  takes the fixture); test, deny and build skipped;
- ticket 292 on `7fb5874`: fmt 0, clippy 101 (E0618 in the rewritten seed host test: a local named
  `owned` shadowed the function it called again); the rest skipped;
- ticket 299 on `b316492`: fmt 0, clippy 0, test 101 (1083 passed, 1 failed: an existing CLI test
  still asserted the generic message for a missing `main`, which fix 5 replaces), deny 0, build 0;
- ticket 306 on `932c8f5` (2026-11-20, 04:12 to 04:17 UTC): every step exited 0, but it does not
  count: its target directory was shared, and its log shows only `gate` compiled from this checkout;
- ticket 320 on `7fb79d5` (the code of `932c8f5` plus this file; 07:00 to 07:19 UTC), in this task's
  own target directory: fmt 0, clippy 0, test 0 (1084 passed, 0 failed), deny 0, build 0. Its log
  compiles `gate`, `gate-host`, `gate-runtime`, `gate-sdk`, `gate-plugin-agents`, `gate-plugin-docs`
  and `gate-plugin-ledger` from `T-062/code`, and lists each test of the table below by name, passing.

Kill check, ticket 321 on `932c8f5` (07:19 to 07:22 UTC; ticket 307 ran the same mutations but its
log kept no compile lines), `T-062/lane/mutants.sh` in a scratch worktree of that commit, in this
task's own target directory; its log compiles each mutated crate from that worktree: three groups of mutations, each removing pieces the new tests guard, then
only those tests run. Every named test failed with its piece removed:

| Group | Piece removed | Test that failed |
| --- | --- | --- |
| A | the cross-generator seed refusal in `plan` | `sync::a_seed_under_a_glob_another_generator_owns_is_refused_before_anything_is_written` |
| A | the plugin options in the generate key | `a_generators_stored_output_is_keyed_by_the_plugins_options` |
| A | the missing-option message (it returns nothing) | `a_missing_required_plugin_option_says_where_to_set_it_and_its_values` |
| A | the clash by stem (back to the exact name) | `layout::a_declared_extension_that_breaks_the_rules_is_a_configuration_error_naming_the_key` |
| A | the repository scope of `agents/no-claude-md` | `a_nested_project_that_sets_no_claude_md_is_a_configuration_error` |
| A | the `delegated-missing` aspect and the delegated rows of the seed | `layout::a_listed_delegated_entry_is_linked_by_the_seed_and_must_be_tracked`, `lay_down::the_index_seed_lists_the_delegated_entries_the_options_name_after_the_seeded_ones`, `the_docs_index_seeds_and_requires_the_delegated_entries_of_each_kind` |
| B | seeds skipped by `gate sync --check` | `sync::a_seed_filled_and_committed_is_never_drift_never_rewritten_and_never_stale` |
| B | the library's required `releases/` | `standard::only_a_library_requires_an_optional_entry_releases_and_says_why`, `lay_down::an_empty_project_gets_the_required_indexes_of_its_kind_as_seeds` |
| B | the root's absent `ledger/` | `a_root_listed_as_a_project_checks_its_docs_as_a_monorepo_root_and_a_member_its_own_kind` |
| C | seeds left out of `produced` | `main_history_judges_an_edit_of_a_seeded_page_as_the_projects_own_change` |
| C | the `missing` aspect | `layout::each_required_entry_of_each_kind_absent_alone_gives_its_one_missing_finding` |

Fix 2's scenario, as the review wrote it (pages seeded under another generator's glob, filled,
committed, then the seeding generator removed), cannot arise any more: the first `gate sync` with
both generators is refused at the seed step, before anything is written or removed, so no page is
ever deleted. The host test covers that refusal with a person's page in place.

## Resume point

2026-11-19: design written from `main` at `ec66737`. The lead approved the mechanics at `adec0b1`: the
docs plugin and its rule, the standard as a constant, `extra`, reserved names, seeds, no manifest, and
delegated `agents/` and `ledger/`. The revision folds in the owner's two additions (a standard per kind;
the kinds alike, each difference with its reason), the root ledger checked against the rules that apply
at the root, `CLAUDE.md` stated only as the memory page says, and T-063 as the follow-up. Waiting for
the lead's decision on the revision before any code.

2026-11-19: the lead approved the design at `569ab76` for code in v0.3.0, with a fifth kind `library`,
the frontend sections as drafted, and T-063 as the follow-up. Code on `task/T-062-docs-plugin` from
`main` at `ec66737`. Next: the interface (seed, `plugin-options`), the host's seeds, `plugins/docs`,
`agents/no-claude-md` at repository scope, then one lane round.

2026-11-19: code at `1563722` on `task/T-062-docs-plugin`: the interface at `gate:plugin@0.3.0`
(`seed`, `plugin-options`), seeds in `gate sync` and `--check`, `plugins/docs` (the five kinds, the
rule, the generator, their pages, the alikeness tests), `agents/no-claude-md` at repository scope,
the architecture, guide and changelog entries, and one end-to-end test per kind through the built
component. The library's architecture index opens with "System shape", as every kind's does. Next:
lane round ticket 266 on the pushed tip; its exit codes go in a Proof section.

2026-11-20: round ticket 268 on `6f4019d` green in all five steps (Proof above). The lead's preset
rule added: `docs/recommended` and `docs/adoption`, with their tests. Next: one lane round on the
preset commit, then the lead's review.

2026-11-20: ticket 278 on `92b2b6f` green in all five steps. Next: the lead's review and merge.

2026-11-20: the review's fix round is in, rebased onto `main` at `e9e1434`: ten points and the
`delegated` option. Lane ticket 306 on `932c8f5` is green (1084 passed), and kill check 307 shows
each new test failing with its piece removed. Waiting for the lead.

2026-11-20: the lead's lane check (a green counts only when its log compiles the changed crates
from this checkout): 306 and 307 shared a target directory and do not count. Requeued in this
task's own target: round 320 green and kill check 321 with all 14 tests failing, both verified by
log (Proof above).

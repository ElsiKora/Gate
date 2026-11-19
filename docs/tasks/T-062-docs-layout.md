# T-062: One documentation layout for every project

| Field | Value |
| --- | --- |
| Status | design, waiting for the lead's decision; no code |
| Release | v0.3.0 (it changes the plugin interface, which v0.3.0 moves to `gate:plugin@0.3.0`) |
| Writer | "Gate 005" |
| Reviewer | a separate review session |
| Depends on | the `gate:plugin@0.3.0` bump (T-057 or T-059, whichever merges first); T-061 for the delegated entries |
| Touches | `wit/gate.wit` (`generated-file`), `crates/gate-sdk` (a seed in a generator's result), `crates/gate-host/src/sync` (writing and checking seeds), a new `plugins/docs` (one rule, one generator, their pages), `Cargo.toml` (the workspace member), `plugins/agents` (`no-claude-md`: scope and page), `docs/architecture.md` sections 4 and 10, `docs/guide/plugins.md`, `CHANGELOG.md` |

## Goal

Every project's `docs/` has the same top-level entries, each with one job. `gate sync` lays the base
layout down in a project that lacks it and never overwrites a file a person wrote. A project adds an
entry of its own only by declaring it, with its job, in its `gate.toml`. One rule, `docs/layout`,
checks that the layout holds: a required entry missing, an entry nobody declared, a declared entry
missing, an index that does not list an entry, a laid-down file never filled in. The agents and ledger
plugins keep what they own inside the layout, and there is no second list of their files.

## The trees read for this design

Four trees were read, read-only, by listing their tracked files and reading files at `HEAD`. Nothing
was run in them, and no name, path or text of theirs is in this repository. In general terms:

- **A large service book.** About 420 files under 14 top-level entries: an index and a start page; a
  glossary; decision records; agent rules and skills; a one-file "AI" directory holding a reading map
  for agents; the ledger; a chapter per code module; reference contracts; runbooks; testing rules. It
  also has three directories that each hold only an index page: an architecture page of real content,
  and two link tables ("concepts", "how-to") that point into the other entries.
- **Two copies of a smaller service book**, one older than the other. They have the same skeleton,
  smaller: index, start page, decisions, agents, the one-file "AI" directory, ledger, modules,
  reference.
- **The repository-level book of a monorepo.** It is a different kind: numbered guide chapters with
  several hundred example data files, architecture notes with two tab-separated data files, research
  notes and release notes. Its decisions and follow-ups live as two long files inside its
  architecture directory. It has no index, no start page, no decision records and no ledger.

The entry names that recur, and what each tree does with them, decide the standard below. Entries with
a real job stay. A directory that only re-indexes other entries goes, because the index and the start
page already do that job.

## 1. The standard layout

A project's documentation is `docs/` at the project root. The layout governs only the top-level
entries of `docs/`. What a directory entry holds below its index is the entry's own business, so data
files, examples and nested directories are allowed there.

**Required.** Every project has these. `gate sync` lays them down when they are missing.

| Entry | Holds |
| --- | --- |
| `README.md` | The index: what the tree holds and the reading order. It links every top-level entry. |
| `start-here.md` | The first reading path for a newcomer, human or agent: the mental model in a page and where to go next. |
| `architecture/` | How the system is built and why: its parts, boundaries and dependency direction. `README.md` is the index. |
| `adr/` | Decision records: `README.md` (the log, with the record template) and one `NNNN-<slug>.md` per decision. |
| `reference/` | Exact values and contracts: formats, vectors, inventories, the glossary (`reference/glossary.md`). `README.md` is the index. |

**Delegated.** These are known names, and their plugins own them. `docs/layout` never requires them,
never lays them down and never looks inside them.

| Entry | Owner | What decides its content |
| --- | --- | --- |
| `agents/` | the `agents` plugin (`root` = `docs/agents`) | Its rules: `rules/*.md`, `skills/`, and since T-061 an `#input` finding when the rule files are missing. The agents' reading map lives here as `agents/context-map.md`. |
| `ledger/` | the `ledger` plugin | The ledger manifest (`ledger/manifest.json`): its canonical files and identifier families. Since T-061, every ledger rule reports a missing input. |

**Optional.** These are known names, each with one job. A project has one only when it has that job.
`gate sync` never lays them down.

| Entry | Holds |
| --- | --- |
| `guide/` | Task-oriented chapters for the people who use or change the project: how-to pages and tutorials, with their examples. |
| `modules/` | A chapter per code module, and the module registry. |
| `operations/` | Runbooks and production procedures. |
| `testing/` | The test taxonomy and the rules for what counts as evidence. |
| `research/` | Exploration that informed a decision but is not one. |
| `releases/` | One page per released version: what a consumer changes. |

**Reserved.** These names may not be used, not even as a declared extension, because the job they
name already has an entry. The finding names where the content goes.

| Reserved | Goes to |
| --- | --- |
| `concepts`, `explanation` | `architecture/` |
| `how-to`, `howto`, `tutorials`, `guides` | `guide/` |
| `ai` | `agents/` |
| `decisions` | `adr/` (decision bodies the ledger keeps stay in `ledger/`) |
| `glossary.md` | `reference/glossary.md` |

**One form per entry.** A directory entry is a directory whose index is `README.md`; a file entry is a
Markdown file. `architecture.md` in place of `architecture/` is not the same entry; it is an unknown
one. Dot entries (`.gitkeep`, editor files) are unknown too, so one rule covers every name.

**The four trees mapped.**

| Tree | Stays | Moves | Goes | Is added (laid down, then filled) |
| --- | --- | --- | --- | --- |
| Large service book | index, start page, `adr/`, `agents/`, `ledger/`, `modules/`, `reference/`, `operations/`, `testing/`, `architecture/` | the glossary into `reference/`; the "AI" reading map into `agents/context-map.md` | the "concepts" link table, folded into the index's table of documentation types; the "how-to" link table, which becomes `guide/README.md` or folds into the index | nothing |
| Smaller service books (both copies) | index, start page, `adr/`, `agents/`, `ledger/`, `modules/`, `reference/` | the "AI" reading map into `agents/context-map.md` | nothing | `architecture/` |
| Repository-level book | `architecture/`, `guide/`, `research/`, `releases/` | the decisions file into `adr/` records; the follow-ups file into a ledger, or into an extension the project declares | nothing | `README.md`, `start-here.md`, `adr/`, `reference/` |

The moves are each project's own work after this task. This task only makes them checkable.

## 2. Where the layout is declared

There are two sources, one for each kind of fact:

- **The standard**, meaning the required, delegated, optional and reserved names and the one form of
  each, is a constant of the `docs` plugin, released with it. It is the same for every project. A
  project cannot rename, drop or redefine a standard entry, so there is nothing to configure about it.
  This is what "one layout" means.
- **A project's extensions** are the `extra` option of `docs/layout`, in the project's `gate.toml`:

  ```toml
  [rules."docs/layout"]
  level = "error"
  options = { extra = [{ name = "playbooks", job = "Step-by-step plays for incidents nobody has automated yet" }] }
  ```

  The rules for an extension:
  - `name` is kebab-case. It is a directory (with `README.md`) or a Markdown file (`name` ending in
    `.md`).
  - It is not a standard, delegated or reserved name.
  - `job` is a sentence that says what the entry holds.

  Breaking any of these is a configuration error (exit 2) at load, with the key named, as for any
  invalid option. A declared entry that does not exist is a finding (`#extra-missing`), so a
  declaration cannot outlive its directory.

Why not a manifest file like the ledger's? The ledger manifest exists because a real ledger is mostly
the project's own files, which no plugin default could name (T-061, question 1). The documentation
layout is the other way round: almost all of it is standard, and a project adds a line or two. Options
in `gate.toml` are validated against a schema, layered, and versioned with the rest of the
configuration. A second JSON file in `docs/` would be one more file to keep well formed, for a list
that is usually empty.

**No second list.** The layout names `agents/` and `ledger/` and nothing inside them. The ledger
manifest stays the one list of ledger files (T-061), and the agents plugin's rules stay the one source
for agent files. A file inside a delegated entry is never a `docs/layout` finding, and a missing
delegated entry is a finding of its owner's rules only. So one missing file never gets a finding from
both plugins.

## 3. The command that lays it down

`gate sync`, through a generator of the `docs` plugin, `docs/layout`. No new command.

Today a generator's file is entirely its own: `gate sync` writes it whenever its contents differ, and
`gate sync --check` reports any difference as drift. That cannot lay down a page a person then fills,
because the next `gate sync` would put the empty page back. So the interface gets one more kind of
generated file, the **seed**:

```wit
/// A file a generator writes, at a path relative to the project root; the host maps it. A seed is
/// written only when nothing exists at its path; after that it belongs to the project.
record generated-file { path: string, contents: list<u8>, seed: bool }
```

| | An owned file (`seed: false`, today) | A seed (`seed: true`) |
| --- | --- | --- |
| `gate sync` | writes it when the contents differ | writes it only when nothing exists at the path in the working tree (no file, link or directory, tracked or not); otherwise leaves it alone |
| `gate sync --check` | reports it missing or different (`gate/sync#<generator>`) | never reports it: a seed claims nothing about contents; whether the file must exist is a rule's question |
| owned globs | a tracked file under them that no generator writes is stale | a seed under a glob its generator owns is refused: the plugin fails the call as an environment error, because a seed belongs to the project from the moment it is written |
| reported after `gate sync` | among the written files, and the untracked ones to `git add` | the same |

The seed never compares contents, so a person's edit is never overwritten. Because the check is
whether the path exists, an untracked file someone just wrote is not overwritten either. A seed is
written again only after its file is deleted. The generator seeds exactly the required entries, so a
required file deleted on purpose comes back, and that is the point of "required".

**On an empty project** (with `docs/layout` in `[generate]` and no `docs/`), `gate sync` writes five
files and reports them as untracked:

- `docs/README.md`: the index. It has a table that links the five required entries, with one line per
  entry saying what goes there, and the line `<!-- gate:seed: replace this page's text, then delete this line -->`.
- `docs/start-here.md`, `docs/architecture/README.md`, `docs/reference/README.md`: each a heading, one
  paragraph saying what the entry holds, and the same marker line.
- `docs/adr/README.md`: the log's heading, an empty log table, the record template as a fenced block,
  and the marker line.

There is no separate template file. A seeded `0000-template.md` would come back every time someone
deleted it, and it is not a required entry.

**On an existing project**, it writes only the required paths where nothing exists. On the smaller
service books above, that is `docs/architecture/README.md` and nothing else. On the large service
book, it writes nothing. It never deletes, never touches an optional, delegated or extension entry, and
never touches a file that is already there.

**`gate sync --check`** does not report seeds. A missing required entry is a finding of `docs/layout`
in `gate check`, once, and `gate sync` is its fix. This keeps the T-061 principle of one finding per
missing input, from the rule that reads it.

The generator has no options. It needs no tree reads, since it decides nothing from the tree; the core
decides whether to write.

## 4. The rule that checks it

`docs/layout`, project scope. It reads the tracked paths under `docs/` and the text of `docs/README.md`
and of the required indexes. It does not read history. Its aspects:

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `missing` | a required entry is not tracked (for a directory entry, its `README.md`) | the entry's path | the entry |
| `unknown` | a top-level entry is not required, delegated, optional or declared. For a reserved name, the message says where its content goes. | the entry's path | the entry |
| `extra-missing` | a declared extension is not tracked | `docs/` | the declared name |
| `entry-index` | a directory entry that is required, optional or declared has no `README.md`. Delegated entries are their owners' business. | the directory | the entry |
| `index` | `docs/README.md` has no local link to a tracked top-level entry, delegated ones included. Links are resolved as `markdown/local-links` resolves them. | `docs/README.md` | the entry |
| `unfilled` | a tracked file of a required entry still holds the seed marker line | the marker's line | the file |

Example findings:

```text
error docs/layout#unknown docs/concepts
  docs/concepts is not an entry of the documentation layout; its content belongs in docs/architecture
error docs/layout#missing docs/start-here.md
  docs/start-here.md is a required entry of the documentation layout; run gate sync to lay it down, then fill it
error docs/layout#unfilled docs/architecture/README.md:7
  docs/architecture/README.md is still the page gate sync laid down; write the architecture, then delete the marker line
```

The level is the configuration's, as always. A project that is migrating can set
`levels = { unfilled = "warning" }`, or use exceptions, like any other aspect. The rule has one option,
`extra`.

## 5. Which plugin owns it

A new plugin, `docs`, with the rule `docs/layout` and the generator `docs/layout`. The others do not
fit:

- **`agents`** owns one entry of the layout (`agents/`) and the files Claude Code reads (`.claude/`,
  `AGENTS.md`). If it owned the whole layout, the ledger's entry would sit under the agents plugin, and
  a project without coding agents could not have a checked layout.
- **`ledger`** owns one entry (`ledger/`) and its manifest. If it owned the whole layout, a project
  without a ledger could not have a checked layout. Its manifest describes files inside `ledger/`, not
  the tree around it.
- **`markdown`** checks what is inside Markdown files anywhere in a project: front matter, sections,
  links. The layout is about the shape of one directory, and it has a generator. Folding it into
  `markdown` would make a project that only wants its Markdown checked take on the layout too.

A separate plugin keeps each plugin to one subject. A project turns the layout on or off on its own,
and the layout knows its two delegated entries only by name. The cost is one more plugin to build, pin
and release. The plugin release workflow already takes any `plugins/<name>` by its tag.

## 6. `CLAUDE.md`

**Decision: a `CLAUDE.md` stays forbidden, at every level, with no exception. `agents/no-claude-md`
moves to repository scope, so that a root file is always seen.**

Claude Code's documentation (code.claude.com/docs/en/memory, read 2026-11-19) settles the question:

- Claude Code reads `AGENTS.md` itself as the project's instructions, with no `CLAUDE.md`, import or
  setting. A nested `AGENTS.md` loads when Claude opens a file in that directory.
- It does this "only when you have no `CLAUDE.md` in your working directory or above it". When both
  exist, it reads the `CLAUDE.md` files only.

So a root `CLAUDE.md` that is exactly `@AGENTS.md` does not make Claude Code read `AGENTS.md`; it
already does that. The root file switches Claude Code to its `CLAUDE.md`-only mode for every directory
below it. The root `AGENTS.md` still arrives through the import, but a nested project's `AGENTS.md`
that has no `CLAUDE.md` beside it is no longer read in a session that starts there. The rule's message,
"a tracked CLAUDE.md disables AGENTS.md", is literally what the documentation describes.

The alternatives, and why not:

- **Allowing a file that is exactly `@AGENTS.md`** is an exception by its contents. It still hides
  every nested `AGENTS.md`, unless each nested project also has its own import file.
- **Requiring one, generated next to every `AGENTS.md`** (the `agents/adapters` generator writing
  `CLAUDE.md` = `@AGENTS.md`, and owning `**/CLAUDE.md`) would be one clean rule. But it makes every
  repository carry a file that current Claude Code does not need. It exists only for clients older than
  the version that reads `AGENTS.md` itself, and it ties the layout to one tool's switch. The client's
  version is not something a repository should encode.

The hole is real: a monorepo whose root `CLAUDE.md` only imports `AGENTS.md` has the rule enabled only
for its nested projects. A project-scope rule there cannot see the root file, and the root file is
exactly what hides the nested `AGENTS.md`. So:

- `agents/no-claude-md` becomes a repository-scope rule. It runs once, sees every tracked path of the
  repository, and reports every `CLAUDE.md` in any case.
- A repository-scope rule is configured at the repository root, so the hole closes only when the root
  configuration enables it. Its page says so, and so does `docs/guide/plugins.md`. A nested project
  that names it in its own `gate.toml` gets the configuration error the core already gives for any
  repository-scope rule set below the root.
- The `.claude/rules` and `.claude/skills` adapters stay as they are. Claude Code reads them whether or
  not a `CLAUDE.md` exists, and they carry the rule files, not the entry.
- `CHANGELOG.md` (0.3.0, breaking): `agents/no-claude-md` moves from the projects' `gate.toml` files to
  the root one.

## 7. Tests

Native SDK tests per rule aspect and for the generator, core tests for the seed, and one end-to-end run
through the built components. Each case is one that would let a broken change through, or block a good
one.

**Seeds (core, `crates/gate-host/src/sync`):**

- A seed whose path is absent is written, and it is listed as written and untracked.
- A seed whose path holds a tracked file with other text is not written. The same holds for an
  untracked file, a directory at the path, and a symbolic link at the path.
- `gate sync --check` with a seed absent reports nothing for it, and with the seed's file edited
  reports nothing.
- An owned file next to a seed still reports drift as today.
- A seed under its generator's owned glob fails the call as an environment error naming the path. The
  same output marked owned passes.
- A seed is never stale: deleting the generator from `[generate]` does not remove a seeded file.

**Generator `docs/layout`:**

- An empty project gets exactly the five files, each with the marker line, all seeds.
- A project with every required file gets five seeds, none of which `gate sync` writes, and an empty
  diff after `gate sync`.
- A project missing only `docs/architecture/README.md` gets that one file written.

**Rule `docs/layout`, one fixture per aspect, each with a passing twin:**

- `missing`:
  - each of the five required entries absent, one at a time, gives one finding at its path;
  - `docs/architecture/` holding files but no `README.md` is `missing`, not `entry-index`;
  - the complete layout gives none.
- `unknown`:
  - `docs/concepts/` names `docs/architecture`, `docs/ai/` names `docs/agents`, and
    `docs/glossary.md` names `docs/reference/glossary.md`;
  - `docs/notes/`, `docs/architecture.md` and `docs/.gitkeep` are each `unknown`;
  - each optional and delegated name passes;
  - a nested path (`docs/guide/examples/x.json`, `docs/architecture/map.tsv`) is never `unknown`.
- `extra`:
  - a declared `playbooks/` with `README.md` passes;
  - declared but absent gives `extra-missing`;
  - a declared `concepts`, `ledger` or `Playbooks` exits 2, naming the key;
  - a declared `playbooks.md` file passes.
- `entry-index`: `docs/operations/` with pages and no `README.md` is reported. `docs/ledger/` and
  `docs/agents/` without one are not.
- `index`:
  - an index that links every present entry passes;
  - a present `docs/research/` that the index does not link is reported;
  - a link to `docs/research/README.md` and a link to `research/` both count.
- `unfilled`:
  - the seeded pages give one finding each, at the marker line;
  - deleting the line clears it;
  - the marker in a file outside the required entries is not reported.
- **Delegated entries:** with `docs/ledger/` and `docs/agents/` absent, `docs/layout` reports nothing
  about them. The T-061 `#input` findings remain the only ones.

**`agents/no-claude-md`, repository scope:**

- A root `CLAUDE.md` holding only `@AGENTS.md` is reported, and so is a nested `x/CLAUDE.md` and a
  `claude.md` in lower case.
- A repository with neither passes.
- Naming the rule in a nested project's `gate.toml` is a configuration error.

**End to end:** an empty repository with the `docs` plugin pinned, `docs/layout` in `[rules]` and
`[generate]`:

- `gate check` exits 1 with five `missing` findings;
- `gate sync` writes five files;
- `gate check` then exits 1 with five `unfilled` findings, one per seeded page;
- after the marker lines are removed (and the index already links the five entries), `gate check`
  exits 0 and `gate sync --check` exits 0.

**Keys:** every finding is keyed by its entry or file, so editing text around it never changes its
fingerprint.

## Done when

- The lead decides on questions 1 to 6. Then the code goes on the tip that carries
  `gate:plugin@0.3.0`, followed by one lane round on the pushed tip: fmt, clippy, test
  (`--workspace --no-fail-fast`), deny and the release build exit 0, and the leak scan prints nothing.
- Do not edit docs/plan.md unless the lead asks.

## Resume point

2026-11-19: design written from `main` at `ec66737`; waiting for the lead's decision before any code.

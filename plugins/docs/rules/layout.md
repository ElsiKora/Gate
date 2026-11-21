# `docs/layout`

A project's `docs/` follows the documentation standard of its kind.

## What it checks and why

A reader who knows one project's documentation should find their way in any other. So every
project's `docs/` follows one standard per kind of project, and the standards look alike: the same
top-level names, the same index files, the same order, and the same place for the same kind of
content. They differ only where a kind's nature demands it, and each difference has its reason
below.

The kind is the `kind` of the `[plugins.docs]` options; the generator
[`docs/layout`](../generators/layout.md) reads the same line:

```toml
[plugins]
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "service" } }
```

`kind` is one of `service`, `frontend`, `infrastructure`, `library` and `monorepo-root`. It is
required and never inferred: a missing or unknown kind is a configuration error (exit 2).

The rule reads the tracked paths under `docs/` and the text of the indexes. It never reads inside
`agents/` or `ledger/`, whose plugins own what is there; of a delegated entry the project lists, it
checks only that some file is tracked under it.

Scope: project. Reads history: no. Aspects: `missing`, `unknown`, `extra-missing`,
`delegated-missing`, `entry-index`, `index`, `sections`, `unfilled`.

### The entries

In the standard order, which is also the order of the index's entry table. **same** means the entry
exists with this job in the kind, as in the others; **absent** means the kind has no such entry, for
the reason given.

| Entry | Job | Service | Frontend | Infrastructure | Library | Monorepo root |
| --- | --- | --- | --- | --- | --- | --- |
| `README.md` | the index: the reading order and a table of every entry | same | same | same | same | same |
| `start-here.md` | the first reading path, for a person or an agent | same | same | same | same | same |
| `architecture/` | how the system is built and why | same | same | same | same | same |
| `modules/` | a chapter per owned part of the code: a module, a feature, a stack, a package | same | same | same | same | absent: the root owns no code; its parts are member projects, each with its own `docs/`, and the root's index links them under Members |
| `adr/` | decision records: the log and one `NNNN-<slug>.md` per decision | same | same | same | same | same |
| `reference/` | exact values and contracts: formats, inventories, settings, the glossary | same | same | same | same | same |
| `guide/` | task-oriented chapters: how to do one thing, with examples | same | same | same | same | same |
| `testing/` | what the tests are, where they live, what counts as evidence | same | same | same | same | same |
| `operations/` | runbooks: deploy, roll back, recover, rotate, release | same | same | same | same | same |
| `agents/` | agent rules, skills and the agents' reading map; owned by the `agents` plugin | same | same | same | same | same |
| `ledger/` | work records, verification, follow-ups; owned by the `ledger` plugin | same | same | same | same | absent: work is recorded in the ledger of the member project whose code it changes, and a repository-wide decision is an `adr/` record |
| `research/` | exploration that informed a decision but is not one; optional | same | same | same | same | same |
| `releases/` | one page per released version: what a consumer changes; optional | same | same | same | required: publishing versions is a library's nature, and the consumer of each version needs the page that says what to change | same |

A directory entry's index is its `README.md`. `research/` and `releases/` are present only when the
project has that job, except that a library requires `releases/`; **required** marks that cell. `agents/` and `ledger/` are delegated: their own plugins' rules require what is
inside them, so this rule requires them only when the project lists them (see below).

**Reserved names** are refused, even as a declared extension, because their job has an entry. A name
without `.md` is reserved as a directory and as a Markdown file.

| Reserved | Goes to |
| --- | --- |
| `concepts`, `explanation` | `docs/architecture/` |
| `how-to`, `howto`, `tutorials`, `guides` | `docs/guide/` |
| `components`, `features`, `stacks`, `packages` | `docs/modules/` |
| `ai` | `docs/agents/` |
| `decisions` | `docs/adr/` |
| `runbooks` | `docs/operations/` |
| `glossary.md` | `docs/reference/glossary.md` |

### The sections of each index

Each index has these level-2 sections, in this order. Other sections may follow.

| Index | Service | Frontend | Infrastructure | Library | Monorepo root |
| --- | --- | --- | --- | --- | --- |
| `README.md` | Reading order, Entries | same | same | same | Reading order, Entries, Members |
| `start-here.md` | Mental model, First day | same | same | same | same |
| `architecture/README.md` | System shape, API surface, Data, Runtime | System shape, Routing, State, Rendering, API access | System shape, Environments, Networking, Secrets and parameters | System shape, Public API, Versioning and release, Compatibility | System shape, Members and dependencies, Build and release flow |
| `modules/README.md` | Chapter map, Adding a module | same | same | same | (no entry) |
| `adr/README.md` | Log, Template | same | same | same | same |
| `reference/README.md` | Contents | same | same | same | same |
| `guide/README.md` | Pages | same | same | same | same |
| `testing/README.md` | Kinds of test, Evidence | same | same | same | same |
| `operations/README.md` | Runbooks | same | same | same | same |
| `releases/README.md` | Versions | same | same | same | same |

The reasons for the differences:

- **The root's `README.md` adds Members:** a root's parts are member projects, and a reader needs a
  table that links each member's `docs/README.md`.
- **Each kind's architecture index:** what "how the system is built" covers is the kind's nature. A
  service has an API and the data behind it; a frontend has routes, state and a rendering model;
  infrastructure has environments, a network and the secrets that cross it; a library has a public
  API that others build on, a release line and the versions it supports; a root has members and the
  order in which they build. "System shape" opens every one.

### A project's own entries

A project adds an entry the standard does not have only by declaring it, with its job, in the
`extra` key of the same options:

```toml
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "service", extra = [{ name = "playbooks", job = "Plays for incidents nobody has automated yet" }] } }
```

`name` is kebab-case: a directory (with its `README.md`), or a Markdown file ending in `.md`. It
reuses no entry of the standard, of any kind, by name or stem, in any case (`start-here`, `readme`,
`ledger.md` are refused), and is not a reserved name; `job` is not empty. Breaking any
of these is a configuration error (exit 2) that names the key. Extensions come after the standard's
entries in the index's order, in the order declared.

### The delegated entries

A project that enables the `agents` or the `ledger` plugin lists the entry in the `delegated` key of
the same options:

```toml
docs = { source = "ElsiKora/Gate:docs@0.3.0", options = { kind = "service", delegated = ["agents", "ledger"] } }
```

The seed of `docs/README.md` then has a row for each entry listed, linking its directory, after the
seeded entries; the index must link each one listed; and each one listed must be tracked, which its
own plugin's `gate sync` does. A delegated directory that exists needs its row in the index whether
it is listed or not. The default is empty. A name that is not a delegated entry, an entry the kind
does not have (`ledger` in a monorepo root) or a name listed twice is a configuration error
(exit 2) that names the key.

### The monorepo root

A path belongs to the project with the longest root that contains it, so a root `docs/` is checked
only when the workspace lists `"."` as a project, with `kind = "monorepo-root"`. That project enables
no ledger rule: each member project keeps its own ledger. If it does, the two plugins disagree in
plain sight (the ledger's rules want a ledger, and this rule reports `docs/ledger/` as `unknown`);
the fix is the root's configuration.

## Options

None of its own. It reads `kind`, `extra` and `delegated` from the `[plugins.docs]` options. A
preset carries rule options only, so neither preset sets them.

## Presets

`docs/recommended` turns the rule on at error, `docs/adoption` at warning, for a project that is
moving to its standard; `extends = ["docs/adoption"]` in `[project]` or a profile, then `[rules]`
overrides it as usual. The generator is not part of a preset: put `"docs/layout" = {}` in
`[generate]`.

## Findings

| Aspect | Reported when | At | Key |
| --- | --- | --- | --- |
| `missing` | a required entry of the kind is not tracked (for a directory, its `README.md`) | the entry's index | the entry |
| `unknown` | a top-level name is not an entry of the kind and not declared; for a reserved name, a kind's absent entry or a Markdown file named like a directory entry, the message says where the content goes or why the kind has none; the help lists the kind's entries and how to declare one | the name | the name |
| `extra-missing` | a declared extension is not tracked | the declared name | the name |
| `delegated-missing` | a delegated entry listed in `delegated` has no tracked file under it | the entry's name | the entry |
| `entry-index` | an optional or declared directory has no `README.md` | its `README.md` | the entry |
| `index` | the Entries section of `docs/README.md` does not link a present entry or a listed delegated entry, or lists the entries out of the standard order | `docs/README.md` | `link <entry>`, `order <entry>` |
| `sections` | an index lacks required sections of its kind (one finding naming every absent one, in the standard's order), or has them out of order; the help gives the order | the index (the first misplaced heading, for the order) | `<index>#missing`, `<index>#order` |
| `unfilled` | a required index still holds the seed's marker line | the marker's line | the index |

```text
error docs/layout#unknown docs/ledger
  docs/ledger/ is not an entry of a monorepo root: work is recorded in the ledger of the member project whose code it changes, and a repository-wide decision is an adr/ record
  help: the entries of a monorepo root are docs/README.md, docs/start-here.md, ...; declare another in the [plugins.docs] options as extra = [{ name = "<name>", job = "<what it holds>" }]
error docs/layout#missing docs/start-here.md
  docs/start-here.md is a required entry of a service; run gate sync to lay it down, then fill it
error docs/layout#sections docs/architecture/README.md
  docs/architecture/README.md has no section "Routing"; in a frontend it comes after "System shape"
  help: write each as a level-2 heading, ## <title>, in this order: System shape, Routing, ...
error docs/layout#sections docs/start-here.md
  docs/start-here.md lacks the sections "Mental model", "First day"; in a service they are: Mental model, First day
  help: write each as a level-2 heading, ## <title>, in this order: Mental model, First day
```

The target of each finding is the entry's or the index's path. A project that is migrating can set
`levels = { unfilled = "warning" }`, or use exceptions, like any other aspect.

## How to fix it

Run `gate sync` with [`docs/layout`](../generators/layout.md) in `[generate]`: it lays down every
required index that is missing. Then fill each page and delete its marker line. Move the content of
an unknown entry into the entry its message names, or declare the entry with its job. For a listed
delegated entry that is missing, run `gate sync` with its plugin's generator in `[generate]`, or
remove it from `delegated`.

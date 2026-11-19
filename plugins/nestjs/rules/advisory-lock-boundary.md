# `nestjs/advisory-lock-boundary`

Advisory-lock SQL, keys and identifiers stay inside their registered boundaries.

## What it checks and why

An advisory lock taken outside the one executor skips the manifest's ordering and identifiers, and
can deadlock with the locks the executor takes or silently collide with them. The rule finds every
place that runs advisory SQL, or names a registered lock identifier, and holds it to the boundaries the
options name.

It reads the tracked `standalone-files` and the files below `roots` (outside `[languages.typescript] skip-folders`): JavaScript and TypeScript sources (`.cjs`,
`.cts`, `.js`, `.jsx`, `.mjs`, `.mts`, `.ts`, `.tsx`) with their syntax tree, `.sql` files as one
statement, and `.sh` and `.json` files for their shell commands (a `package.json` for its `scripts`).
Each file reports its first finding, in this order:

- `#inventory`: the file holds a registered family or resource identifier of the manifest (a numeric
  literal, or a value a constant expression evaluates to) and is not listed in `inventory`. The
  identifiers of `contextual-resources` (too small to stand for their resource, such as `1`) are left
  out when the list matches the manifest exactly; otherwise the mismatch is reported at the options
  file. A manifest that cannot be read or holds no identifier is reported at the manifest. Off without
  `inventory`.
- `#mint`: a `mint.required` file lacks one of its texts (the finding is the path and the entry's
  `message`), or a file outside `mint.structural` builds an object literal with every one of
  `mint-properties`. Off without `mint`.
- `#shell`: a shell command runs one of `shell-executables` with advisory SQL (a `pg_` and an advisory
  lock or unlock in its letters, after its static variables are expanded).
- `#unresolved`: the argument of a SQL sink (a call of one of `sql-sinks.methods`, or a template tagged
  with one of `sql-sinks.tags`, through aliases, `bind`, `call` and `apply`) shows advisory evidence
  but cannot be rebuilt statically, or a reconstruction exceeds its budget.
- `#sql`: the SQL cannot be read.
- `#derivation`: a statement derives a key another way (`hashtext`, `bit(64)` and the like).
- `#executor`: a call in the executor is not one of `advisory-functions`, qualified with `pg_catalog`,
  with `$1::integer, $2::integer`.
- `#migration`: a `migrations` file holds other advisory calls than one
  `pg_catalog.pg_advisory_xact_lock` of the `ledger` key's identifiers. Non-empty `migrations` without
  `ledger` is reported at the options file; a `ledger` the manifest lacks, at the manifest.
- `#negative`: the `negative` file holds other advisory calls than one
  `pg_catalog.pg_advisory_xact_lock($1::bigint)`.
- `#outside`: any other file runs advisory SQL.
- `#unreadable`: a source does not parse or is not UTF-8 text; a SQL, JSON or shell file that is not
  UTF-8 is read with replacement characters and reported besides its first finding.
- `#boundary`: the executor is not tracked, or a migration or the negative test holds no advisory
  statement.

Findings are reported at the file. Two identical findings are reported once.

Scope: project. Language: `typescript`. Reads history: no. Aspects:
`boundary`, `derivation`, `executor`, `inventory`, `migration`, `mint`, `negative`, `outside`,
`shell`, `sql`, `unreadable`, `unresolved`. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `manifest-path` | path | `layout.source-root` + `/common/advisory-lock/registry.manifest.json` | The registry manifest, as `nestjs/advisory-lock-manifest` reads it. |
| `roots` | list of paths | `layout.source-root`, `layout.test-root`, `layout.scripts-root` | The folders whose files are read. |
| `standalone-files` | list of paths | `["package.json"]` | The files outside the roots that are read. |
| `executor` | path | required | The one file that runs advisory SQL. Without it the options are an error; leave the rule off instead. |
| `migrations` | list of paths | `[]` | Baseline migrations that lock the `ledger` key. |
| `negative` | path | none | The negative test. |
| `ledger` | `FAMILY.RESOURCE` | none | The static key the baseline migrations lock. |
| `contextual-resources` | list of `{ group, name, id }` | none | Resources whose identifiers leave the inventory. |
| `inventory` | list of paths | none | The files that may hold registered identifiers. |
| `mint` | `{ structural, required: [{ path, texts, message }] }` | none | The files that may build structural keys, and the texts mint files keep. |
| `advisory-functions` | list of strings | the ten `pg_advisory_*` and `pg_try_advisory_*` lock and unlock functions | The functions the executor may call. |
| `sql-sinks` | `{ methods, tags }` | `{ methods = ["addSelect", "query", "select"], tags = ["sql"] }` | The calls and tags whose argument is SQL. |
| `shell-executables` | list of strings | `["execute", "psql", "query"]` | The executables that run SQL from a shell. |
| `mint-properties` | list of strings | `["family", "familyId", "resourceKey"]` | The properties of a structural key. |

```toml
[rules]
"nestjs/advisory-lock-boundary" = { level = "error", options = { executor = "src/common/advisory-lock/function/execute.function.ts" } }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

```text
error nestjs/advisory-lock-boundary#outside src/stock/rebuild.ts
  src/stock/rebuild.ts contains executable advisory SQL outside the exact allowlist.
```

## How to fix it

Take advisory locks only through the executor with a key from the manifest, keep registered identifiers
in the files `inventory` lists, and build keys only in the mint files.

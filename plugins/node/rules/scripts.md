# `node/scripts`

A project calls each job by one script name: a bare name checks, and `<name>:fix` writes.

## What it checks and why

When one project runs its type check as `typecheck`, another as `type-check` and a third as
`lint:types`, nobody can run a job across projects without looking it up, and CI needs a list per
project. When `lint` also rewrites files, a check in CI changes the tree it checks. The rule gives
each job one name, keeps the writing variant under `<name>:fix`, and requires the scripts every
project of the kind has.

The rule reads the `scripts` of the `package.json` at the project root; a project without one is
left to `node/package`. Two facts of the project decide what is required:

- the project is a workspace member when no `pnpm-lock.yaml` is tracked at its root (an install
  root keeps its lockfile there, see `node/package-manager`);
- the project uses TypeScript when it tracks a file whose name starts with `tsconfig` and ends
  with `.json`, at any depth.

For each script:

- **`#synonym`**: the name is not a synonym of a standard name. The synonyms are `lint:types`,
  `type-check`, `tsc` and `check-types` for `typecheck`; `format:check`, `prettier:check` and
  `prettier` for `format`; `prettier:fix`, `prettier:write` and `format:write` for `format:fix`;
  `lint:ts` and `eslint` for `lint`; `lint:ts:fix` and `eslint:fix` for `lint:fix`.
- **`#writes`**: a script whose name does not end in `:fix` has no `--fix` or `--write` word in its
  command (`--fix=…` counts; `--fix-type` does not), and no `-w` given to Prettier (a `-w` after a
  word ending in `prettier` in the same command, before any `&&`, `||`, `;` or `|`; `tsc -w`
  watches and does not count).
- **`#misplaced`**: a workspace member has no `format` or `format:fix` script, because "formatting
  is repository-wide, with one configuration at the root; the root's `format` and `format:fix`
  cover every member".
- **`#name`**: the name is lower-case kebab-case segments joined by `:` (`test:e2e`,
  `build-storybook`); `prepublishOnly`, an npm lifecycle name, keeps its spelling.

And for the project:

- **`#missing`**: the project has `build`, `lint`, `lint:fix` and `test`; `typecheck` when it
  uses TypeScript; `format` and `format:fix` when it is not a workspace member; `dev` when its kind
  is `frontend`. A missing script is reported at the line of the `scripts` key, or at the file
  when there is none.

The kinds are `service`, `frontend`, `infrastructure`, `library` and `monorepo-root`. They share
every script but one row of the standard's table of differences (`DIFFERENCES` in
`src/standard.rs`): a `frontend` has "a dev script", because "a development server is part of a
frontend's job; the other kinds have no single such command". No other kind requires `dev`, and
none forbids it. The names, synonyms and flags are fixed by the plugin's standard; none is
configurable.

Whether the project uses TypeScript is decided by its own `tsconfig*.json` files: a file below a
directory with its own `package.json` belongs to that package, not to the project.

Scope: project. Language: none. Reads history: no. Aspects: `missing`, `synonym`, `writes`,
`misplaced`, `name`.

## Options

The rule has no options of its own. It reads `kind` from `[plugins.node] options`, set in each
project's configuration.

```toml
[plugins.node.options]
kind = "frontend"

[rules]
"node/scripts" = { level = "error" }
```

## Example finding

`apps/web/package.json`, a project of kind `frontend` with a `tsconfig.json`, in a workspace:

```json
{
  "name": "@scope/web",
  "scripts": {
    "build": "vite build",
    "lint": "eslint . --fix",
    "lint:fix": "eslint . --fix",
    "typecheck": "tsc --noEmit",
    "test": "vitest run"
  }
}
```

```text
error node/scripts#writes apps/web/package.json:5
  script lint rewrites files (eslint . --fix), and only a <name>:fix script writes
  help: make it check only, and put the writing command in <name>:fix
```

```text
error node/scripts#missing apps/web/package.json:3
  package.json has no script dev; a frontend has it: a development server is part of a frontend's
  job; the other kinds have no single such command
  help: add the script under its standard name
```

The message names the path relative to the project root. The finding's target is
`<path>:<script name>`, the path relative to the project root: `package.json:lint`,
`package.json:dev`.

## How to fix it

Rename a synonym to its standard name and a non-kebab name to kebab-case. Make a bare script check
only (`eslint .`, `prettier --check .`) and put the writing command in its `:fix` script. Add the
missing standard scripts. In a workspace member, remove `format` and `format:fix`; the root's
scripts format every member.

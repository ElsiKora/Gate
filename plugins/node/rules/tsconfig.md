# `node/tsconfig`

Every `tsconfig` of the project reaches the repository's one base through `extends`, none weakens
it, and module resolution is the kind's.

## What it checks and why

The strict flags are what make TypeScript catch the errors it is used for. When each project copies
them, the copies drift; when a configuration turns one off, a whole project loses a check without a
trace in review. The rule keeps the flags in one base per repository, makes every configuration
inherit them, and lets a project set only the module resolution its kind needs.

The base is the file the option `base` names, a path from the repository root. The standard flags,
which the base sets to `true`, are `strict`, `noUncheckedIndexedAccess`,
`exactOptionalPropertyTypes`, `noImplicitOverride`, `noImplicitReturns`,
`noFallthroughCasesInSwitch`, `useUnknownInCatchVariables`, `forceConsistentCasingInFileNames`,
`isolatedModules` and `verbatimModuleSyntax`. The flags `strict` turns on are `noImplicitAny`,
`strictNullChecks`, `strictFunctionTypes`, `strictBindCallApply`, `strictPropertyInitialization`,
`noImplicitThis` and `alwaysStrict`.

The rule reads every tracked file of the project whose name starts with `tsconfig` and ends with
`.json`, at any depth, as JSON with comments and trailing commas. A project without one has nothing
to check. For each configuration:

- **`#weakened`**: no standard flag, and no flag `strict` turns on, is set to `false` in
  `compilerOptions`. The base is checked too.

For each configuration but the base:

- **`#base`**: it reaches the base through `extends`, directly or through other configurations of
  the project. An `extends` (a string or a list of strings) is followed when it starts with `.`; it
  is resolved from the configuration's directory as TypeScript resolves it: the path as written,
  then with `.json` added, then, for a directory, its `tsconfig.json`; the first that exists is
  the target. A package (`@tsconfig/node24/tsconfig.json`) names an installed package, which is
  not in the tree, and is not followed. It is reported at the `extends` line, or at
  the file when there is none.
- **`#copy`**: a configuration without `extends` does not set three or more standard flags to
  `true`; one that does repeats the base instead of extending it.
- **`#resolution`**: `module` and `moduleResolution`, where the configuration sets them, are the
  kind's, compared without regard to case: `ESNext` and `Bundler` for a `frontend`, `NodeNext` and
  `NodeNext` for every other kind.

For a `frontend` with a `tsconfig.json` at the project root, that file also sets
`moduleResolution`, itself or through a configuration of the project it extends (followed as
above) other than the base the project owns; otherwise it inherits the base's, and it is reported
under `#resolution` at the file.

When the base lies inside the project (the project that owns it, such as the repository root):

- **`#flags`**: the base is tracked, sets every standard flag to `true`, and sets `module` and
  `moduleResolution` to `NodeNext`.

The kinds are `service`, `frontend`, `infrastructure`, `library` and `monorepo-root`. They share
every flag; they differ in one row of the standard's table of differences (`DIFFERENCES` in
`src/standard.rs`): a `frontend` has "module ESNext and moduleResolution Bundler", because "a
bundler resolves a frontend's imports, so Bundler is the resolution that matches what runs". Every
other kind keeps the base's `NodeNext`. The flags and the resolutions are fixed by the plugin's
standard; only the base's path is configurable.

The rule reads the project's own `tsconfig*.json` files: a file below a directory with its own
`package.json` belongs to that package, which is checked as its own project against its own kind.
A project at the repository root therefore leaves its members' configurations to them.

Scope: project. Language: none. Reads history: no. Aspects: `base`, `weakened`, `flags`,
`resolution`, `copy`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `base` | string | `"tsconfig.base.json"` | The base's path from the repository root. |

The rule reads `kind` from `[plugins.node] options`, set in each project's configuration.

```toml
[rules]
"node/tsconfig" = { level = "error", options = { base = "config/tsconfig.base.json" } }
```

## Example finding

`apps/api/tsconfig.json`, a project of kind `service`:

```json
{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "strictNullChecks": false,
    "moduleResolution": "Bundler"
  }
}
```

```text
error node/tsconfig#weakened apps/api/tsconfig.json:4
  tsconfig.json turns strictNullChecks off, which the standard keeps on
  help: remove the line; an exception with its reason is the way to relax a flag for a file
```

```text
error node/tsconfig#resolution apps/api/tsconfig.json:5
  tsconfig.json sets moduleResolution to "Bundler"; a service uses NodeNext
  help: write the kind's value, or leave it to the base where the kind's is the base's
```

The message names the path relative to the project root. The finding's target is `<path>:<key>`,
the path relative to the project root, where the key is the flag or the file name:
`tsconfig.json:strictNullChecks`, `tsconfig.json:moduleResolution`,
`tsconfig.build.json:tsconfig.build.json`.

## How to fix it

Keep the standard flags and `NodeNext` in the base, and start every configuration from it:
`"extends": "../../tsconfig.base.json"`, or a configuration of the project that does. Remove the
flags a configuration copies from the base and every line that turns a standard flag off; a file
that needs a flag relaxed gets an exception with its reason. A frontend sets
`"module": "ESNext"` and `"moduleResolution": "Bundler"` in its own `tsconfig.json`; the other
kinds leave both to the base.

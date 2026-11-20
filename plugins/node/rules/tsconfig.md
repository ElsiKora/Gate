# `node/tsconfig`

Every `tsconfig` of the project reaches its install root's base through `extends`, none weakens
it, and module resolution is the kind's.

## What it checks and why

The strict flags are what make TypeScript catch the errors it is used for. When each project copies
them, the copies drift; when a configuration turns one off, a whole project loses a check without a
trace in review. The flags live in one base per install root, which
[`node/tsconfig-base`](tsconfig-base.md) checks; this rule makes every configuration of a project
inherit them, so no project escapes the flags by not extending, and lets a project set only the
module resolution its kind needs.

The base is the file the plugin option `base` names (default `tsconfig.base.json`), a path from the
install root that holds the project: the repository root, or the directory of the `separate` entry
the project is in, whose base is its own. The standard flags, which the base sets to `true`, are
`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `noImplicitOverride`,
`noImplicitReturns`,
`noFallthroughCasesInSwitch`, `useUnknownInCatchVariables`, `forceConsistentCasingInFileNames`,
`isolatedModules` and `verbatimModuleSyntax`. The flags `strict` turns on are `noImplicitAny`,
`strictNullChecks`, `strictFunctionTypes`, `strictBindCallApply`, `strictPropertyInitialization`,
`noImplicitThis` and `alwaysStrict`.

The rule reads every tracked file of the project whose name starts with `tsconfig` and ends with
`.json`, at any depth, as JSON with comments and trailing commas. A project without one has nothing
to check. The base itself, when it is one of the project's files, is not read here:
[`node/tsconfig-base`](tsconfig-base.md) checks it once per install, from the repository. For each
other configuration:

- **`#weakened`**: no standard flag, and no flag `strict` turns on, is set to `false` in
  `compilerOptions`.
- **`#resolution`**: `module` and `moduleResolution`, where the configuration sets them, are the
  kind's, compared without regard to case: `ESNext` and `Bundler` for a `frontend`, `NodeNext` and
  `NodeNext` for every other kind.

A solution-style configuration, whose `files` is an empty list and which has no `include`
(Vite's `tsconfig.json` with only `references`), compiles nothing: it is not read for `#base` and
`#copy`, and the configurations it refers to are checked instead, as their own files. For each
other configuration:

- **`#base`**: it reaches the base through `extends`, directly or through other configurations of
  the project. An `extends` (a string or a list of strings) is followed when it starts with `.`; it
  is resolved from the configuration's directory as TypeScript resolves it: the path as written,
  then with `.json` added, then, for a directory, its `tsconfig.json`; the first that exists is
  the target. A package (`@tsconfig/node24/tsconfig.json`) names an installed package, which is
  not in the tree, and is not followed. It is reported at the `extends` line, or at
  the file when there is none. When the chain goes through a configuration outside the project
  that is not the base, the project cannot read it, so the chain cannot be followed: the message
  names the configuration it leaves through, and the help gives the `extends` that names the base
  directly.
- **`#copy`**: a configuration without `extends` does not set three or more standard flags to
  `true`; one that does repeats the base instead of extending it.

For a `frontend` with a `tsconfig.json` at the project root, that file also sets
`moduleResolution`, itself or through a configuration of the project it extends (followed as
above) other than the base; otherwise it inherits the base's, and it is reported under
`#resolution` at the file. When that `tsconfig.json` is solution-style, each configuration of the
project it refers to (`references[].path`, a file or a directory with a `tsconfig.json`) sets it
instead, and the one that does not is reported.

The kinds are `service`, `frontend`, `infrastructure`, `library` and `monorepo-root`. They share
every flag; they differ in one row of the standard's table of differences (`DIFFERENCES` in
`src/standard.rs`): a `frontend` has "module ESNext and moduleResolution Bundler", because "a
bundler resolves a frontend's imports, so Bundler is the resolution that matches what runs". Every
other kind keeps the base's `NodeNext`. The flags and the resolutions are fixed by the plugin's
standard; only the base's path is configurable, in `[plugins.node] options`.

The rule reads the project's own `tsconfig*.json` files: a file below a directory with its own
`package.json` belongs to that package, which is checked as its own project against its own kind.
A project at the repository root therefore leaves its members' configurations to them. A file a
`reads` grant adds from outside the project's root is not the project's own and is not read.

Scope: project. Language: none. Reads history: no. Aspects: `base`, `weakened`, `resolution`,
`copy`.

## Options

The rule has no options of its own. It reads `kind`, `base` and `separate` from
`[plugins.node] options`, set in each project's configuration:

```toml
[plugins.node]
source = "ElsiKora/Gate:node@0.3.0"
options = { kind = "service", base = "config/tsconfig.base.json" }

[rules]
"node/tsconfig" = "error"
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

`apps/api/tsconfig.json` with `"extends": "../../packages/config/tsconfig.json"`, a configuration
outside the project:

```text
error node/tsconfig#base apps/api/tsconfig.json:1
  tsconfig.json extends through packages/config/tsconfig.json, outside the project, so its chain to
  tsconfig.base.json cannot be followed
  help: extend the base directly: "extends": "../../tsconfig.base.json"
```

## How to fix it

Keep the standard flags and `NodeNext` in the base, and start every configuration from it:
`"extends": "../../tsconfig.base.json"`, or a configuration of the project that does. Remove the
flags a configuration copies from the base and every line that turns a standard flag off; a file
that needs a flag relaxed gets an exception with its reason. A frontend sets
`"module": "ESNext"` and `"moduleResolution": "Bundler"` in its own `tsconfig.json`; the other
kinds leave both to the base.

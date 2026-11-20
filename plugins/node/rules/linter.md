# `node/linter`

A repository that uses ESLint has one flat configuration, `eslint.config.mjs` at its root, built on
the shared configuration; a configuration below the root imports it.

## What it checks and why

ESLint 9 and later read only the flat configuration; a legacy `.eslintrc` file or an
`eslintConfig` key is ignored and only looks like a rule set. A configuration below the root that
does not start from the root's, or a root that mixes several base configurations, lets the rules
of one directory drift from the rest. The rule keeps one root configuration on one shared base, and
every directory configuration on top of the root's.

A repository uses ESLint when a `package.json` (outside `node_modules`) lists `eslint` in
`dependencies` or `devDependencies`, or when any legacy or flat configuration below is tracked.
Then:

- **`#legacy`**: no `eslintConfig` key in any `package.json`, and no file named `.eslintrc` or
  `.eslintrc.<ext>` (`js`, `cjs`, `mjs`, `json`, `yaml`, `yml`) anywhere.
- **`#root`**: `eslint.config.mjs` is tracked at the repository root, and every flat configuration
  (`eslint.config.<ext>`, where `<ext>` is `js`, `mjs`, `cjs`, `ts`, `mts` or `cts`), at the root
  or below, is named `eslint.config.mjs`.
- **`#shared`**: the root `eslint.config.mjs` imports the shared package (option `shared`) or a
  subpath of it (`<shared>/<path>`), and imports no other ESLint configuration package: a package
  named `eslint-config` or `eslint-config-<name>`, scoped or not. The other package is reported at
  its import's line.
- **`#detached`**: a flat configuration below the root imports the root `eslint.config.mjs` by a
  relative path (`../../eslint.config.mjs` from `apps/web`).

The rule reads the imports from the file's tokens: a string after `from` or `import`, or as the
first argument of `import(…)` or `require(…)`, so static imports, `export … from`, `import()` and
`require()` all count. Comments, strings, template literals with substitutions and regular
expressions are skipped as JavaScript reads them, so an import in a comment or a string does not
count, and neither does a property such as `Array.from("x")`.

The file names and forms are fixed by the plugin; only the shared package is configurable.

Scope: repository. Language: none. Reads history: no. Aspects: `legacy`, `root`, `shared`,
`detached`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `shared` | string | `"@elsikora/eslint-config"` | The shared ESLint package the root imports. |

`[plugins.node] options` must hold a valid `kind` (the options are checked before the rule runs),
but this rule's checks do not depend on `kind` or `separate`.

```toml
[rules]
"node/linter" = { level = "error", options = { shared = "@scope/eslint-config" } }
```

## Example finding

The root `eslint.config.mjs`:

```js
import { createConfig } from "@elsikora/eslint-config";
import extra from "@scope/eslint-config";

export default [...(await createConfig({ withTypescript: true })), ...extra];
```

```text
error node/linter#shared eslint.config.mjs:2
  eslint.config.mjs imports @scope/eslint-config as another base; @elsikora/eslint-config is the
  only one
  help: remove the import; what the repository needs on top goes into its own blocks
```

`apps/web/eslint.config.mjs`:

```js
import { createConfig } from "@elsikora/eslint-config";

export default await createConfig({ withTypescript: true });
```

```text
error node/linter#detached apps/web/eslint.config.mjs
  apps/web/eslint.config.mjs does not import the root eslint.config.mjs, so it can drift from it
  help: import the root configuration by its relative path and add only what this directory needs
```

The finding's target is `<path>:<key>`, where the key is the import's specifier, the file name or
`eslintConfig`: `eslint.config.mjs:@scope/eslint-config`,
`apps/web/eslint.config.mjs:eslint.config.mjs`.

## How to fix it

Write the root `eslint.config.mjs` with the shared package's `createConfig` and add only what the
repository needs in blocks of its own. Rename any other flat configuration to
`eslint.config.mjs`, move a legacy configuration into the flat one and remove it. A directory that
needs more starts from the root's:

```js
import root from "../../eslint.config.mjs";

export default [...root, { files: ["src/**/*.tsx"], rules: {} }];
```

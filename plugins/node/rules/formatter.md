# `node/formatter`

A repository has one Prettier configuration: a root `.prettierrc.json` with the standard
settings.

## What it checks and why

Prettier merges the nearest configuration file with its defaults, so a second file below the
root, a `prettier` key in a `package.json` or a file in another format formats part of the tree
differently, and an editor and CI can disagree on the same file. The rule keeps one configuration,
in one format, at the root, holding the standard and nothing else.

The standard is Prettier's defaults but for three keys:

| Key | Value |
| --- | --- |
| `printWidth` | `120` |
| `proseWrap` | `"preserve"` |
| `trailingComma` | `"all"` |

The values are fixed by the plugin's standard (`src/standard.rs`) and are not configurable. The
shared ESLint configuration `@elsikora/eslint-config` fixes no Prettier option of its own: it
loads the recommended configuration of `eslint-plugin-prettier`, which reads the repository's
Prettier configuration. The plugin therefore holds these three values.

A repository without a tracked `package.json` (outside `node_modules`) is not a Node repository,
and the rule reports nothing. Otherwise:

- **`#form`**: the configuration is `.prettierrc.json` at the repository root. Another
  configuration file at the root (`.prettierrc`, `.prettierrc.<ext>` or `prettier.config.<ext>`,
  where `<ext>` is `json`, `json5`, `yaml`, `yml`, `toml`, `js`, `cjs`, `mjs`, `ts`, `cts` or
  `mts`), a `prettier` key in the root `package.json`, and a missing root `.prettierrc.json` are
  reported.
- **`#nested`**: no Prettier configuration sits below the root: any of the files above in a
  subdirectory, `.prettierrc.json` included, and a `prettier` key in any other `package.json`.
- **`#settings`**: the root `.prettierrc.json` sets each standard key to its value, compared as
  compact JSON (`"120"` is not `120`), and sets no other key but `$schema`.

The rule does not read `.prettierignore`. A directory the plugin option `separate` names is not
exempt: it is formatted by the root configuration too.

Scope: repository. Language: none. Reads history: no. Aspects: `form`, `nested`, `settings`.

## Options

The rule has no options of its own. `[plugins.node] options` must hold a valid `kind` (the options
are checked before the rule runs), but this rule's checks do not depend on `kind` or `separate`.

```toml
[plugins.node.options]
kind = "service"

[rules]
"node/formatter" = { level = "error" }
```

## Example finding

`.prettierrc.json`:

```json
{
  "printWidth": 100,
  "proseWrap": "preserve",
  "trailingComma": "all",
  "semi": false
}
```

```text
error node/formatter#settings .prettierrc.json:2
  .prettierrc.json sets printWidth to 100; the standard is 120
  help: write the standard value; the formatting of every repository is the same
```

```text
error node/formatter#settings .prettierrc.json:5
  .prettierrc.json sets semi, which the standard leaves at Prettier's default
  help: remove the key; the standard sets only printWidth, proseWrap and trailingComma
```

The finding's target is `<path>:<key>`, where the key is the setting, the file name or `prettier`:
`.prettierrc.json:printWidth`, `.prettierrc.json:semi`, `apps/web/.prettierrc:.prettierrc`.

## How to fix it

Keep one `.prettierrc.json` at the repository root:

```json
{
  "$schema": "https://json.schemastore.org/prettierrc",
  "printWidth": 120,
  "proseWrap": "preserve",
  "trailingComma": "all"
}
```

Remove every other Prettier configuration file and every `prettier` key in a `package.json`. Code
that should not be formatted goes into `.prettierignore`, not into a second configuration.

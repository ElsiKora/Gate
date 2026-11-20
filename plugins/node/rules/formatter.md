# `node/formatter`

A repository has one Prettier configuration: a root `.prettierrc.json` with the standard
settings, and no `.editorconfig` that changes what Prettier writes.

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

- **`#editorconfig`**: Prettier also reads `.editorconfig` and maps six of its keys to its own
  options; an option set in `.prettierrc.json` wins over the key. So in every tracked
  `.editorconfig` (outside `node_modules`), in a section whose pattern covers a file Prettier
  formats (a `.js`, `.ts`, `.json`, `.md`, `.yaml`, `.css`, `.html` or similar file; a pattern
  with a directory is matched by its last segment), a key is reported when the option it maps to
  is not in `.prettierrc.json` and the value Prettier takes from it is not the standard's. The
  mapping is Prettier 3.10.1's (`editorconfig-to-prettier.js`, after the editorconfig parser's
  defaults):

  | Key | Prettier option | Reported when |
  | --- | --- | --- |
  | `indent_style` | `useTabs` | it is `tab` |
  | `indent_size` | `useTabs` | it is `tab`, with no `indent_style` and no `tab_width` |
  | `indent_size` | `tabWidth` | it is a number other than `2`, and `indent_style` is `space` or there is no `tab_width` |
  | `tab_width` | `tabWidth` | it is a number other than `2`, unless `indent_style` is `space` with a numeric `indent_size` |
  | `max_line_length` | `printWidth` | it is `off` or a number other than `120` |
  | `quote_type` | `singleQuote` | it is `single` |
  | `end_of_line` | `endOfLine` | it is `crlf` or `cr` |

  Since the standard `.prettierrc.json` sets `printWidth`, `max_line_length` is read only where it
  does not. Each section is read on its own; the fix is to remove the key or write the standard's
  value, and the Prettier configuration stays the one source of the values. A section for files
  Prettier does not format, such as `[Makefile]` with `indent_style = tab`, is not read.

The rule does not read `.prettierignore`. A directory the plugin option `separate` names is not
exempt: it is formatted by the root configuration too.

Scope: repository. Language: none. Reads history: no. Aspects: `form`, `nested`, `settings`,
`editorconfig`.

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

`.editorconfig` with `indent_size = 4` on its line 5, in its `[*]` section:

```text
error node/formatter#editorconfig .editorconfig:5
  .editorconfig sets indent_size = 4 for [*], and Prettier reads it; the standard formats with 2
  help: remove the key or write indent_size = 2; .prettierrc.json is the one source of Prettier's
  settings
```

The finding's target is `<path>:<key>`, where the key is the setting, the file name or `prettier`,
and for `.editorconfig` the section and the key: `.prettierrc.json:printWidth`,
`.prettierrc.json:semi`, `apps/web/.prettierrc:.prettierrc`, `.editorconfig:[*]indent_size`.

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

Remove every other Prettier configuration file and every `prettier` key in a `package.json`, and
every `.editorconfig` key above that disagrees with the standard in a section Prettier reads. Code
that should not be formatted goes into `.prettierignore`, not into a second configuration.

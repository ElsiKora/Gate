# `nestjs/module-registry`

Every concrete native Nest module below the source root has one row of the module registry, whose
chapter maps the module in its source map section.

## What it checks and why

The module registry is the map from code to documentation: one Markdown table row per module, each
linking the chapter that owns the module. A module without a row has no documented owner, a row
without a module is stale, and a chapter that does not name its module's file drifts from the code.

A native module is a class, neither `abstract` nor `declare`, in a tracked `.ts` file (not `.d.ts`)
below `layout.source-root`, outside `[languages.typescript] skip-folders`, decorated with a call of
`module-decorator` exported by `module-package`. The decorator is followed through named, aliased and
namespace imports, `const` aliases, element access, optional calls, parentheses and assertions; a
local function of the same name is not the package's decorator. A file without a syntax tree is
skipped; `typescript/parse` reports it.

The registry is read line by line after fenced code blocks and HTML comments are blanked. A table
starts at a line of `|` and the `table-header` cells (up to three leading spaces), followed by a
delimiter row of three cells of at least three dashes with optional alignment colons; its rows are the
following lines that start with `|`. Every finding is located in `registry`.

- `registry-shape`: the registry exists and has a module table; each header is followed by a
  delimiter row; each row has three non-empty cells, a first cell that is a code span of a `.ts` path
  below the source root without white space, `*`, `|`, or an empty, `.` or `..` segment, and a second
  cell that is exactly one link `[label](target)`; no module path has two rows.
- `registry-coverage`: every native module has a row, and every row names a native module.
- `registry-chapter`: a row's link, resolved from `chapter-root`, is not `chapter-index`; for a
  native module whose chapter lies below `chapter-root`, the chapter exists and its source map section
  (from the line that is `source-map-heading`, to the next heading of the same level) holds a code span
  `` `<source root>/…` `` that matches the module path, where `*` stands for any characters. A link
  outside `chapter-root` is not read.
- `owner-chapter`: each row whose module path lies below the source root links exactly one local
  Markdown chapter below `documentation-root`. For this aspect the registry is read with a CommonMark
  parser (GFM tables, front matter blanked): a module table is any table whose header cells are
  `table-header`, a row's module path is the first code span of its first cell, and its links are the
  links of its second cell (an undefined reference counts as a link without a target, a wiki link
  does not) and every bare URL in it. The one link must be a relative path, resolved from the
  registry's folder, to a `.md` file below `documentation-root`; whether the file exists is a link
  checker's job.

Scope: project. Languages: none, so a change to the registry or a chapter alone runs it. Reads history: no. Aspects: `owner-chapter`,
`registry-chapter`, `registry-coverage`, `registry-shape`. Findings carry no target.

## Options

Path options are relative to the project root and in normal form (`src/book`, not `./src/book/`); any
other form is an options error that shows the normal form.

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `registry` | string | `"docs/modules/module-registry.md"` | The registry page. |
| `chapter-root` | string | `"docs/modules"` | `registry-chapter`, `registry-coverage`: the folder chapter links resolve from; a chapter outside it is not read. |
| `documentation-root` | string | `"docs"` | `owner-chapter`: the folder a row's one chapter link must land in. |
| `chapter-index` | string | `"docs/modules/README.md"` | The index of the chapters, which no row may link. |
| `table-header` | array of three strings | `["Module Path", "Chapter", "Ownership"]` | The header cells of the module table. |
| `source-map-heading` | string | `"## Source Map"` | The heading of a chapter's source map section. |
| `module-package` | string | absent: the layout's package (`layout.nest-common-package`) | The package of the module decorator. |
| `module-decorator` | string | `"Module"` | The module decorator. |

The source root is `layout.source-root` of `[plugins.nestjs] options`. `registry`, `table-header` and
the source root apply to every aspect.

```toml
[rules]
"nestjs/module-registry" = { level = "error", options = { registry = "docs/modules/module-registry.md" } }
```

## Example finding

```text
error nestjs/module-registry#registry-coverage docs/modules/module-registry.md
  docs/modules/module-registry.md is missing source module: src/book/book.module.ts
error nestjs/module-registry#owner-chapter docs/modules/module-registry.md:7
  docs/modules/module-registry.md src/book/book.module.ts must link one local documentation chapter
```

## How to fix it

Add one row per module to the registry table, with the module file as a code span, a link to the
module's chapter and its owner. List the module file, or a pattern with `*` that matches it, in the
chapter's source map section. Remove rows of modules that no longer exist. Give every row exactly
one relative link to its chapter below `documentation-root`.

# `agents/instruction-references`

Backticked paths and commands in agent instructions exist.

## What it checks and why

An agent follows the instructions literally: a path that does not exist or a script that the
package does not define sends it astray. The rule reads the entry file and every `*.md` below
`root`, and judges each inline code span:

- `#command`: a span that matches `command-pattern` names, in the pattern's first group, a script of
  the `scripts` of `package-json`. The form of a command belongs to the project's toolchain, so
  `command-pattern` has no default and this aspect is checked only when it is set.
- `#path`: a span that names a path must be tracked (a file or a directory), match a tracked path or
  a parent directory when it is a glob, be the name (or, starting with `.`, the suffix) of some
  tracked file when it has no slash, or be ignored by the project's tracked `.gitignore`. A
  `:line[:column]` suffix is dropped first. In a project of a workspace, a path not found in the
  project is also looked up from the repository root, among the paths `[plugins.agents] reads`
  grants the project: `infrastructure/cdk` cited from `apps/server` exists when `reads` grants
  `infrastructure/cdk` (or a directory above it) and some tracked file lies below it. The project's
  `.gitignore` applies only to a path read inside the project (a bare name, a path that starts with
  `.` or with a top-level name of the project). A path read from the root counts when it is a
  granted tracked path, or when the repository's root `.gitignore` ignores it and `reads` grants
  that file; without that grant only tracked files count from the root.

A span is prose, not a path, when it is empty, contains whitespace, `://`, `...`, or one of
`<>{}$|()=`, starts with `-`, `./` or `../`, or (with a slash) its first segment is neither a
top-level name of the project or of a granted path outside it nor starts with `.`, or (without a slash) it does not end with one of
`extensions`, starts with `*`, or is only an extension such as `.md`.

`#input`: `entry` is not tracked, reported at its path and keyed by it. The files below `root` are
still checked. A project without its entry file fails the rule: create it, point `entry` at the
file, or turn the input off with `levels = { input = "off" }` and a `decision`.

Scope: project. Reads history: no. Aspects: `command`, `input`, `path`.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `root` | string | `"docs/agents"` | The directory whose `*.md` files are read, relative to the project. |
| `entry` | string | `"AGENTS.md"` | The entry file; reported as `#input` while it is not tracked. |
| `extensions` | array of string | `[".md", ".mdc", ".mjs", ".cjs", ".js", ".ts", ".json", ".toml", ".yml", ".yaml", ".sh", ".txt"]` | The endings of a bare file name that make it a path. |
| `package-json` | string | `"package.json"` | The file whose `scripts` a command must name; read only with `command-pattern`. |
| `command-pattern` | string (regular expression) | none | A command, with the script name in its first group; without it, commands are not checked. A pattern that does not compile or has no group is a configuration error. |

```toml
[rules]
"agents/instruction-references" = { level = "error", options = { command-pattern = '^npm run ([A-Za-z0-9:_.-]+)(?:\s|$)' } }
```

```toml
[plugins.agents]
source = "ElsiKora/Gate:agents@0.3.0"
reads = ["infrastructure/cdk", ".github/workflows"]
```

## Example finding

`AGENTS.md:12` says `` `npm run lint:all` `` and the package has no such script:

```text
error agents/instruction-references#command AGENTS.md:12
  `npm run lint:all` names no package.json script
```

The finding's target is the script name or the path.

## How to fix it

Correct the path or the command, add the script, or write the text as prose without backticks.

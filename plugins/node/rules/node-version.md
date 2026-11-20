# `node/node-version`

Each install root runs one Node version, written once in `.nvmrc` at that root, and every
`engines.node` below it admits it.

## What it checks and why

When the Node version is written in several places, the places drift: a developer, a container
and CI each run a different Node, and a bug shows up on one of them only. The rule keeps one
place per install, `.nvmrc` at its install root, which version managers and CI setups read, and
makes the declared `engines.node` ranges agree with it. An install root is the repository root, plus
each directory the plugin option `separate` names.

A repository without a tracked `package.json` (outside `node_modules`) is not a Node repository,
and the rule reports nothing. Otherwise:

- **`#file`**: `.nvmrc` is tracked at each install root that has a `package.json` below it, and
  holds one exact version `x.y.z`, optionally with a pre-release or build part
  (`24.11.0-rc.1`), as every rule of the plugin reads an exact version, with nothing else but
  surrounding white space. `24`, `v24.11.0` and `lts/*` are reported; `v24.11.0` is what
  `node --version` prints, and the help says to drop the `v`.
- **`#form`**: no second place names the Node version: a `.node-version` file anywhere, an
  `.nvmrc` anywhere but an install root, a `.tool-versions` file with a line for `nodejs` or
  `node`, a `volta.node` field in any `package.json`.
- **`#engines`**: the install root's `package.json` is tracked and declares `engines.node` as a
  string, which reads as a range. When that root's `.nvmrc` holds an exact version, the range
  admits that version and no version of an earlier
  major. The rule reads comparators (`>=`, `>`, `<=`, `<`, `=`), partial versions (`24`, `24.x`),
  caret, tilde, `*`, hyphen ranges (`20.0.0 - 24`) and alternatives joined by `||`; a range it
  cannot read (`latest`) is reported. `>=22` with `.nvmrc` at `24.11.0` admits Node 22 and is
  reported; `>=24.11.0 <25` and `^24.11.0` pass.
- **`#differs`**: every other `package.json` below an install root that declares `engines.node`
  declares that root's range, the same text once runs of white space are collapsed.

A directory declared in `separate` is an install of its own, as in every rule of the plugin: its
own `.nvmrc` names its Node version, and its `engines.node` agrees with that, not with the
repository root's. The places the rule reads are fixed by the plugin; none is configurable.

Scope: repository. Language: none. Reads history: no. Aspects: `file`, `form`, `engines`,
`differs`.

## Options

The rule has no options of its own. `[plugins.node] options` must hold a valid `kind` (the options
are checked before the rule runs), but this rule's checks do not depend on `kind` or `separate`.

```toml
[plugins.node.options]
kind = "monorepo-root"

[rules]
"node/node-version" = { level = "error" }
```

## Example finding

`.nvmrc` holds `24.11.0`, and the root `package.json`:

```json
{
  "name": "example",
  "private": true,
  "engines": { "node": ">=22" }
}
```

```text
error node/node-version#engines package.json:4
  package.json engines.node ">=22" admits a Node major below 24, the major in its .nvmrc
  help: write a range from the .nvmrc version to the next major, such as ">=24.11.0 <25"
```

With the root range at `>=24.11.0 <25`, `apps/api/package.json` declares `"node": ">=24"` on its
line 3:

```text
error node/node-version#differs apps/api/package.json:3
  apps/api/package.json declares engines.node ">=24", package.json ">=24.11.0 <25"
  help: write the root's range
```

The finding's target is `<path>:<key>`, where the key is the field or the file name:
`package.json:engines.node`, `apps/api/package.json:engines.node`, `.nvmrc:.nvmrc`.

## How to fix it

Write the exact version, such as `24.11.0`, as the only line of `.nvmrc` at the root, and remove
`.node-version`, nested `.nvmrc` files, the Node line of `.tool-versions` and `volta.node`. In the
root `package.json`, write a range from that version to the next major: `">=24.11.0 <25"`. Give
every other `package.json` that declares `engines.node` the same range. When the Node version
changes, change `.nvmrc` and the ranges in one commit.

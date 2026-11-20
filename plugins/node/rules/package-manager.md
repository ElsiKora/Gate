# `node/package-manager`

A repository installs with one package manager, pnpm, pinned by an exact version at each install
root.

## What it checks and why

Two package managers on one tree write two lockfiles that drift apart, and a range in
`packageManager` lets two machines install with two versions of pnpm. The rule makes every install
of the repository the same: pnpm, at one exact version, from one tracked lockfile per install root.

An install root is the repository root, plus each directory the plugin option `separate` names (a
spike or a load test that is an install of its own). A repository without a tracked `package.json`
(outside `node_modules`) is not a Node repository, and the rule reports nothing. Otherwise, at each
install root:

- **`#field`**: the install root's `package.json` is tracked and its `packageManager` is
  `pnpm@<x.y.z>`, an exact version (a pre-release or build part allowed, as every rule of the
  plugin reads an exact version), optionally followed by
  `+<hash>` (as Corepack writes it). A range (`pnpm@^9`), a partial version (`pnpm@9.1`) or
  another manager (`npm@10.0.0`) is reported at the field's line; a missing field or a missing
  `package.json` is reported at the file.
- **`#save-exact`**: the install root's `.npmrc` has a line `save-exact=true` (spaces around the
  key and the value are allowed), so an install writes an exact version, not a range.
- **`#lockfile`**: the install root's `pnpm-lock.yaml` is tracked.

And anywhere in the repository:

- **`#lockfile`**: no tracked file is the lockfile of another package manager:
  `package-lock.json`, `npm-shrinkwrap.json`, `yarn.lock`, `bun.lock`, `bun.lockb`.
- **`#nested`**: a `pnpm-lock.yaml` or `pnpm-workspace.yaml` sits only in an install root. A
  workspace has one of each, at its root; one below it is reported unless its directory is
  declared in `separate`.
- **`#members`**: every tracked `package.json` below an install root (outside `node_modules`, not
  the root's own) matches a glob of `packages` in that root's `pnpm-workspace.yaml`, so an install
  covers it. A manifest belongs to the deepest install root that holds it. Globs are matched
  against the directory relative to the install root; `*` does not cross `/`, a leading `./` and
  a trailing `/` are ignored, and a glob that starts with `!` excludes. Without a
  `pnpm-workspace.yaml`, every manifest below the root is reported.

The package manager, its version form, the `save-exact` setting and the list of other lockfiles
are fixed by the plugin's standard (`src/standard.rs`); none is configurable.

Scope: repository. Language: none. Reads history: no. Aspects: `field`, `lockfile`, `nested`,
`members`, `save-exact`.

## Options

The rule has no options of its own. It reads `separate` from `[plugins.node] options`: each entry
is a `path` (a directory relative to the repository root, without `.`, `..`, a leading or a
trailing slash) and a `job` (one sentence on why it is an install of its own). `kind` is required
in the same table, but this rule's checks do not depend on it.

```toml
[plugins.node.options]
kind = "monorepo-root"
separate = [{ path = "spikes/load", job = "a load test that pins its own dependencies" }]

[rules]
"node/package-manager" = { level = "error" }
```

## Example finding

The root `package.json`:

```json
{
  "name": "example",
  "private": true,
  "packageManager": "pnpm@^9"
}
```

```text
error node/package-manager#field package.json:4
  package.json pins "packageManager": "pnpm@^9", not pnpm at one exact version
  help: write "packageManager": "pnpm@<x.y.z>" with the exact version every install uses
```

`tools/release/package.json`, where the root `pnpm-workspace.yaml` lists `apps/*` and
`packages/*`:

```text
error node/package-manager#members tools/release/package.json
  tools/release/package.json is in no package glob of pnpm-workspace.yaml, so no install covers it
  help: add its directory, or a glob that matches it, to packages in pnpm-workspace.yaml
```

The finding's target is `<path>:<key>`, where the key is the field, the file name or
`package.json`: `package.json:packageManager`, `tools/release/package.json:package.json`.

## How to fix it

Write `"packageManager": "pnpm@<x.y.z>"` in the install root's `package.json`, add
`save-exact=true` to its `.npmrc`, run `pnpm install` there and commit `pnpm-lock.yaml`. Remove the
lockfiles of other package managers. A directory with its own lockfile or workspace file either
joins the root workspace (list it in `packages` and remove the files) or is declared in
`separate` with its job, and then follows the same checks as the root. A `package.json` outside
every package glob gets a glob that matches its directory.

A test fixture is no exception: a fixture's `package.json` or lockfile is tracked like any other,
and the only way to keep it is to declare its directory in `separate` with its job ("fixtures of
the installer's tests"), which makes it an install root of its own, checked like any other
install root. There is no other exemption.

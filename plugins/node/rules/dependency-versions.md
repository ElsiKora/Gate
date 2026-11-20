# `node/dependency-versions`

Each dependency is installed at one exact version; in a workspace, every external dependency comes
from the catalog and every workspace package through `workspace:*`.

## What it checks and why

A range in a manifest lets the same commit install different code on two days, and two members
of one repository that take the same package at two versions ship two copies of it. The rule
keeps one written version per dependency. A pnpm workspace has a place for that version, the
catalog in `pnpm-workspace.yaml`; a repository without a workspace writes exact versions in its
manifests and keeps them equal.

The rule reads every tracked `package.json` outside `node_modules` and groups them by install
root: the repository root and each directory the plugin option `separate` names; a manifest
belongs to the deepest install root that holds it. In each manifest it reads `dependencies`,
`devDependencies` and `optionalDependencies`; `peerDependencies` may hold ranges and are not read.
An exact version is `x.y.z`, optionally with a pre-release or build part (`1.2.3-rc.1`).

When the install root has a `pnpm-workspace.yaml`:

- **`#range`**: every entry of its `catalog`, and of each named catalog under `catalogs`, is one
  exact version. It is reported at the entry's line in `pnpm-workspace.yaml`.
- **`#internal`**: a dependency named like a package of the install root (the `name` of any of
  its manifests), or written with a `workspace:` protocol, is written exactly `workspace:*`.
  `workspace:^` is reported, and so is a `workspace:` dependency that no package of the install
  root is named.
- **`#catalog`**: every other dependency is written `catalog:` or `catalog:<name>`. When the
  workspace file has neither `catalog` nor `catalogs`, every such dependency is reported,
  `catalog:` ones included. Otherwise the catalog a reference names holds an entry for the
  dependency: `catalog:` and `catalog:default` name the `catalog` table (or `catalogs.default`),
  `catalog:<name>` names `catalogs.<name>`; a reference to a catalog without that entry is
  reported.

When the install root has no `pnpm-workspace.yaml`:

- **`#range`**: every dependency is one exact version; `^1.2.3`, `~1.2.3`, `*`, `latest` and a
  non-string value are reported.
- **`#differs`**: one dependency has one version across the install root's manifests. Manifests
  are read in path order; a manifest that takes a dependency at another exact version than the
  first manifest that took it is reported once per entry.

What counts as exact, the sections read and the protocols required are fixed by the plugin; none
is configurable.

Scope: repository. Language: none. Reads history: no. Aspects: `range`, `catalog`, `internal`,
`differs`.

## Options

The rule has no options of its own. It reads `separate` from `[plugins.node] options` to find the
install roots; each is checked on its own, with its own `pnpm-workspace.yaml`. `kind` is required
in the same table, but this rule's checks do not depend on it.

```toml
[plugins.node.options]
kind = "monorepo-root"
separate = [{ path = "spikes/load", job = "a load test that pins its own dependencies" }]

[rules]
"node/dependency-versions" = { level = "error" }
```

## Example finding

`pnpm-workspace.yaml`:

```yaml
packages:
  - "apps/*"
  - "packages/*"
catalog:
  zod: 4.1.12
```

`packages/lib/package.json` is named `@scope/lib`, and `apps/api/package.json`:

```json
{
  "name": "@scope/api",
  "dependencies": {
    "@scope/lib": "workspace:^",
    "zod": "^4.1.0"
  }
}
```

```text
error node/dependency-versions#internal apps/api/package.json:4
  apps/api/package.json dependencies takes the workspace package @scope/lib as "workspace:^", not
  "workspace:*"
  help: write "workspace:*" for a package of the workspace, and a catalog entry for any other
```

```text
error node/dependency-versions#catalog apps/api/package.json:5
  apps/api/package.json dependencies pins zod at "^4.1.0" instead of taking it from the catalog
  help: add the exact version to catalog in pnpm-workspace.yaml and write "catalog:" here
```

The finding's target is `<path>:<dependency name>`: `apps/api/package.json:@scope/lib`,
`apps/api/package.json:zod`. A catalog entry's target is `pnpm-workspace.yaml:<name>`.

## How to fix it

In a workspace, put each external dependency once in the catalog at its exact version
(`zod: 4.1.12`) and write `"zod": "catalog:"` in every manifest that needs it; write
`"@scope/lib": "workspace:*"` for a package of the workspace. Without a workspace, write the exact
version that is installed (`"zod": "4.1.12"`) and the same version in every manifest;
`save-exact=true` in `.npmrc` keeps new installs exact. A peer dependency keeps its range under
`peerDependencies`.

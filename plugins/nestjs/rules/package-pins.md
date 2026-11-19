# `nestjs/package-pins`

Listed packages are exact-pinned to their registry tarballs.

## What it checks and why

Some packages must resolve to one known artifact, for example a package whose error classes the
service matches by identity: two copies, a range or a linked checkout would let a different build in
unnoticed. For each package in `artifacts` the rule reads the npm manifest and lockfile and reports:

- a manifest `dependencies` entry that is not exactly the listed version;
- a lockfile root (`packages[""]`) `dependencies` entry that is not exactly the listed version;
- more than one resolution of the package in the lockfile (a nested `node_modules/<package>`, an entry
  named or resolved as the package), or another dependency name aliasing it (`npm:<package>@…`) in any
  of `alias-sections` of the manifest or the lockfile root;
- a lockfile entry `node_modules/<package>` whose `version`, `resolved` or `integrity` differs from the
  listed artifact, or that is a `link`.

A missing manifest or lockfile is reported even when `artifacts` is empty: at the lockfile when the
manifest exists, at the manifest otherwise. A file that is not valid JSON fails the run.

Scope: project. Languages: none. Reads history: no. Aspects: none. Findings carry no target.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `artifacts` | array of `{ package, version, resolved, integrity }` | `[]` | The packages to pin, each with its exact version, registry tarball URL and integrity hash. |
| `manifest` | string | `"package.json"` | The npm manifest, relative to the project root. |
| `lockfile` | string | `"package-lock.json"` | The npm lockfile, relative to the project root. |
| `alias-sections` | array of string | `["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"]` | The dependency sections in which another name must not alias a listed package. |

```toml
[rules]
"nestjs/package-pins" = { level = "error", options = { artifacts = [
  { package = "@scope/errors", version = "1.2.3", resolved = "https://registry.example.test/@scope/errors/-/errors-1.2.3.tgz", integrity = "sha512-…" },
] } }
```

Every path option is relative to the project root and written in its normalized form: no `./`, `.`
or `..` segment and no doubled `/`; a file or folder has no trailing `/`, a path prefix keeps the one
it is written with. Another form stops the run with an error that shows the normalized form.

## Example finding

`package.json` declares `"@scope/errors": "^1.2.3"`:

```text
error nestjs/package-pins package.json
  @scope/errors must be exact-pinned to 1.2.3 in package.json
  help: pin the exact version in the manifest and reinstall so the lockfile resolves it once, from the
  registry tarball
```

## How to fix it

Declare the exact version in the manifest, remove aliases and links, and reinstall so that the lockfile
holds one `node_modules/<package>` entry with the listed tarball and integrity.

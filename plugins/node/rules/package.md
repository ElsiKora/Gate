# `node/package`

The project's `package.json` has the fields of its kind, and only those.

## What it checks and why

The fields of a `package.json` say what the project is: a module system, the Node it runs on,
whether it may be published and what a consumer gets when it is. When each project picks its own
fields, a service gets published by mistake, a library ships its whole tree, and a tool reads a
field that only some projects have. The rule holds every project to the standard of its kind,
named by the plugin option `kind`.

The rule reads the `package.json` at the project root:

- **`#missing`**: the file exists, and has the fields every kind has: `name`, `version`,
  `license`, `type` and `engines.node` (present; `node/node-version` checks its value). A project
  that is not a library has `private`. A library has `publishConfig`, `exports`, `files`,
  `sideEffects` and `repository`.
- **`#value`**: `type` is `"module"`, in every kind. In a kind other than library, `private` is
  `true`; `false`, `"true"` or any other value is reported.
- **`#forbidden`**: a library does not set `private` to `true`. A kind other than library has none
  of `exports`, `files`, `sideEffects` and `repository`. No kind has `workspaces`:
  `pnpm-workspace.yaml` lists the workspace's packages.

A missing field is reported at the file; a forbidden field or a wrong value at its line.

The kinds are `service`, `frontend`, `infrastructure`, `library` and `monorepo-root`. They share
every field but two rows of the standard's table of differences (`DIFFERENCES` in
`src/standard.rs`); a kind not named in a row has the common cell:

- `private`: a `library` has "no private: true, and a publishConfig", because "a published package
  must not be private, and says where it is published". Every other kind has `private: true`.
- `exports`, `files`, `sideEffects` and `repository`: a `library` has the four fields, because
  "they say what a consumer may import and what the tarball holds; a package that is not
  published has no consumer". Every other kind has none of them.

Every field and value the rule enforces is fixed by the plugin's standard; none is configurable.

Scope: project. Language: none. Reads history: no. Aspects: `missing`, `forbidden`, `value`.

## Options

The rule has no options of its own. It reads `kind` from `[plugins.node] options`, set in each
project's configuration.

```toml
[plugins.node.options]
kind = "library"

[rules]
"node/package" = { level = "error" }
```

## Example finding

`apps/api/package.json`, a project of kind `service`:

```json
{
  "name": "@scope/api",
  "version": "1.0.0",
  "license": "MIT",
  "type": "module",
  "engines": { "node": ">=24.11.0 <25" },
  "files": ["dist"]
}
```

```text
error node/package#missing apps/api/package.json
  package.json has no private; a service has it: a project that is not published is private
  help: add the field
```

```text
error node/package#forbidden apps/api/package.json:7
  package.json has files, which this kind has not: only a library is published; the field
  describes a package for consumers
  help: remove the field
```

The message names the path relative to the project root. The finding's target is
`<path>:<field>`, the path relative to the project root: `package.json:private`,
`package.json:files`; a missing `engines.node` is `package.json:engines.node`.

## How to fix it

Add the fields the kind has and remove the ones it has not. A service, frontend, infrastructure
project or monorepo root is `"private": true` and describes no published package. A library drops
`private`, says where it is published in `publishConfig`, and lists what a consumer may import in
`exports` and what the tarball holds in `files`. Write `"type": "module"` and give a file that must
stay CommonJS the `.cjs` extension. List workspace packages in `pnpm-workspace.yaml`, not in
`workspaces`.

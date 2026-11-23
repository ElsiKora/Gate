# `node/tsconfig-base`

Each install root that uses TypeScript has one base configuration, and the base carries every
standard flag and the common module resolution.

## What it checks and why

The strict flags are kept once, in a base that every configuration extends
([`node/tsconfig`](tsconfig.md) checks that each project's configurations reach it). The base is
then the one file that decides how strict TypeScript is for a whole install, so it is checked
here, at repository scope, once per install, whether or not a Gate project holds it: in a
workspace whose root is not a Gate project, no project owns the root's base, and the base is
still checked.

An install root is the repository root, plus each directory the plugin option `separate` names;
a separate install has a base of its own, checked as a base of its own. The base is the file the
plugin option `base` names (default `tsconfig.base.json`), a path from each install root. An
install root with no tracked `tsconfig*.json` below it (outside `node_modules`, and not under a
deeper install root) does not use TypeScript, and the rule reports nothing for it. Otherwise:

- **`#missing`**: the base is tracked at the install root.
- **`#flags`**: the base sets every standard flag to `true` in `compilerOptions`, and sets `module`
  and `moduleResolution` to `NodeNext`, compared without regard to case. The settings the base
  leaves out are one finding at the file, keyed `compilerOptions`, whose help lists every line to
  add. A setting with another value is a finding of its own at its line, keyed by the setting, so
  a base that turns one flag off on purpose writes it as `false` and holds an exception for that
  flag alone.

The standard flags are `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`,
`noImplicitOverride`, `noImplicitReturns`, `noFallthroughCasesInSwitch`,
`useUnknownInCatchVariables`, `forceConsistentCasingInFileNames`, `isolatedModules` and
`verbatimModuleSyntax`. They and the common resolution are fixed by the plugin's standard and are
the same for every kind; a `frontend` sets its own resolution in its `tsconfig.json`, which
`node/tsconfig` checks. The base is read as JSON with comments and trailing commas, as TypeScript
reads it; its own `extends` is not followed, so the flags are written in the base itself.

Scope: repository. Language: none. Reads history: no. Aspects: `missing`, `flags`.

In a monorepo the base lies outside every project, and a plugin reads only its own project unless
its entry grants more. Each plugin that resolves imports through the project's tsconfig (`nestjs`,
`cdk`, `frontend`, `typescript`) is granted the base with `reads`, written from the repository
root:

```toml
# apps/api/gate.toml
[plugins]
nestjs = { source = "ElsiKora/Gate:nestjs@0.3.1", reads = ["tsconfig.base.json"] }
```

Without the grant the module resolver runs without the base's `paths`, and the plugin reports the
base as a finding with the line to add; see
[configuration.md](../../../docs/guide/configuration.md#languages).

## Options

The rule has no options of its own. It reads `base` and `separate` from `[plugins.node] options`
in the root `gate.toml`:

```toml
[plugins.node]
source = "ElsiKora/Gate:node@0.3.0"

[plugins.node.options]
kind = "monorepo-root"
base = "tsconfig.base.json"
separate = [{ path = "spikes/load", job = "a load test that pins its own dependencies" }]

[rules]
"node/tsconfig-base" = "error"
```

## Example finding

`tsconfig.base.json` without `strict` and `verbatimModuleSyntax`, and with `"module": "ESNext"` on
its line 11:

```text
error node/tsconfig-base#flags tsconfig.base.json
  the base tsconfig.base.json leaves out 2 standard settings: strict, verbatimModuleSyntax
  help: add to its compilerOptions: "strict": true, "verbatimModuleSyntax": true; the base carries every standard flag and the common resolution
```

```text
error node/tsconfig-base#flags tsconfig.base.json:11
  the base tsconfig.base.json does not set module to NodeNext
  help: set it to NodeNext; a frontend overrides it with Bundler in its own tsconfig.json
```

`apps/api/tsconfig.json` is tracked, and the repository root has no `tsconfig.base.json`:

```text
error node/tsconfig-base#missing tsconfig.base.json
  tsconfig.base.json is not tracked; its install root has tsconfig files, and they need a base to
  extend
  help: add the base with the standard flags; every tsconfig of the install extends it
```

The finding's target is `<path>:<key>`, where the key is the flag or the file name:
`tsconfig.base.json:verbatimModuleSyntax`, `tsconfig.base.json:tsconfig.base.json`.

## How to fix it

Write the base at the install root with every standard flag set to `true` and `module` and
`moduleResolution` set to `NodeNext`, and let every configuration of the install extend it. A flag
that a file needs relaxed is relaxed by an exception with its reason, never in the base.

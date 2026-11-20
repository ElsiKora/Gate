# `frontend/packages`

package.json names one tool per job, and the router and translation library of the pack.

## What it checks and why

Two tools for one job mean two ways to do it and two sets of rules to know. The rule reads the
`dependencies` and `devDependencies` of `package.json`; a tool is one or more packages, and a project
uses a tool when it names any of them.

- `job`: two or more tools of one job (`axios` and `ky` for `http-client`). Target
  `package.json#<job>`.
- `pack`: a tool of `router` or `translation` other than the pack's: `tanstack-router` and
  `react-i18next` for `react-vite`, `next` and `next-intl` for `nextjs`, no router and
  `react-i18next` for `react-library`. Target `package.json#<tool>`.

A project without `package.json` is not read; a file that is not JSON stops the run with an error
that names it.

Packs: all three.

Scope: project. Language: `typescript`. Reads history: no. Aspects: `job`, `pack`. Findings
name their path as the target unless the text above names another.

## Options

| Option | Type | Default | Meaning |
| --- | --- | --- | --- |
| `jobs` | map of job to a map of tool to a list of packages | `http-client` (`axios`, `ky`, `ofetch`, `cross-fetch`, `node-fetch`, `isomorphic-fetch`, `undici`, `graphql-request`), `store` (`zustand`, `redux`, `recoil`, `jotai`, `mobx`, `valtio`), `router` (`tanstack-router`, `react-router`, `wouter`, `next`), `translation` (`react-i18next`, `next-intl`, `react-intl`, `lingui`, `typesafe-i18n`), `styling` (`tailwind`, `styled-components`, `emotion`, `sass`, `less`, `vanilla-extract`, `stitches`) | The jobs and their tools. Setting it replaces the whole map; the pack's choices still apply to the jobs `router` and `translation`. |

Every rule also reads the plugin's `pack` ([the plugin's page](README.md)).

## Example finding

```text
error frontend/packages package.json
  package.json names 2 tools for the job http-client: axios, ky
```

## How to fix it

Keep one tool for the job, move its users to it, and remove the others from `package.json`; use the router and translation library of the pack.

# T-091: Gate checks itself with the released 0.3.0

| Field | Value |
| --- | --- |
| Status | in review |
| Release | after `v0.3.0` (configuration and documents only; no version moves) |
| Writer | "Gate 005" |
| Reviewer | the lead reads the change |
| Depends on | `main` at `d2cd512` (the `v0.3.0` tag) |
| Touches | `gate.toml`, `gate.lock`, `.github/workflows/gate.yml` (generated), `docs/plan.md`, this task file |

## Goal

Gate's own `gate.toml` checked the repository with the released 0.2.x core and plugins. With
`v0.3.0` published, it should use the core and plugins it ships, and the plan should record the
release.

## Scope

1. `gate.toml`: `[gate] version = "0.3.0"`, and `git`, `agents`, `markdown` and `ci` at `@0.3.0`;
   `bin/gate lock` (anonymous), `bin/gate sync` and `bin/gate check` exit 0.
2. Whatever 0.3.0 asks of the configuration or reports on the repository is listed below and fixed
   at its cause, never silenced.
3. `docs/plan.md`: the `v0.3.0` row released, with its tag commit and what it holds; T-072, T-077,
   T-080b, T-085, T-086, T-088 and T-088b done; rows for T-090 (done), T-091 (this task) and T-092
   (open).

## What 0.3.0 asked of the configuration

- `bin/gate sync` refused the configuration: `[ci.github] jobs.gate: timeout-minutes is not set`.
  The job now sets `timeout-minutes = 15`. The regenerated workflow also gives every job a timeout
  (`gate-passed` 5), names the checkout step, and moves `contents: read` from the workflow to the
  jobs that check out (`permissions: {}` at the top, and for `gate-passed`).
- `bin/gate check` then reported two `ci/actions-pinned#version-comment` errors, at
  `.github/workflows/gate.yml:25` and `:50`: the checkout pin carried its release as a TOML comment
  after the string, so the generated `uses:` line had none. The release is now part of the pin,
  `"actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1"`, and the workflow carries it.

## Done when

- `bin/gate lock`, `bin/gate sync`, `bin/gate sync --check` and `bin/gate check` exit 0 under 0.3.0.
- The leak scan named in the local rules prints nothing.
- The lead has read the change.

## Proof

- `bin/gate lock`: exit 0, anonymous (no token, no `gh` on `PATH`), resolved the four plugins at
  0.3.0 and removed the 0.2.x pins.
- `bin/gate sync`: exit 0, `4 generated, 1 written` (the workflow); `bin/gate sync --check`: exit 0.
- `bin/gate check`: exit 0, `7 rules, 0 errors, 0 warnings`.
- `git log --merges --first-parent cf84f39..d2cd512`: the merges the plan names; `git tag -l v0.3.0`
  points at `d2cd512`, tagged 2026-11-21T09:08:03Z.

## Resume point

2026-11-21T09:25:30+00:00: written on `task/T-091-after-0.3.0` off `main` at `d2cd512`. Next: the
lead reads the change; no lane round (configuration and documents only).

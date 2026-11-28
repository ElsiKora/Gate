# T-118: Gate checks itself with the released 0.3.5

| Field | Value |
| --- | --- |
| Status | done (merged) |
| Release | after `v0.3.5` (configuration only; no version moves) |
| Writer | the lead (a mechanical pin move) |
| Reviewer | none beyond the lead: the change is the version line and the lock |
| Depends on | `main` at `f500e0c` (T-117 merged; `v0.3.5` released) |
| Touches | `gate.toml`, `gate.lock`, `docs/plan.md`, this task file |

## Goal

Gate's own `gate.toml` uses the core it ships: `[gate] version = "0.3.5"`. Its four plugins, `git`,
`agents`, `markdown` and `ci`, have no release after 0.3.0, so their pins stay. It declares no
`nestjs`, so nestjs 0.3.3 does not concern it. The shape is that of T-112.

## Proof

- `bin/gate lock`: exit 0, `resolved core 0.3.5`, the four plugins kept; `gate.lock` changes only the
  core's version, URLs and sha256.
- `bin/gate sync`: exit 0, `4 generated, 0 written, 0 removed`, so 0.3.5 writes the same workflow and
  hooks as 0.3.4: this repository sets no `[ci.github] cache`.
- `bin/gate check`: exit 0, `7 rules, 0 errors, 0 warnings`.

## Resume point

Done.

# T-110: Gate checks itself with the released 0.3.3

| Field | Value |
| --- | --- |
| Status | done (merged) |
| Release | after `v0.3.3` (configuration only; no version moves) |
| Writer | the lead (a mechanical pin move) |
| Reviewer | none beyond the lead: the change is the version line and the lock |
| Depends on | `main` at `991c6c4` (T-108 merged; `v0.3.3` released) |
| Touches | `gate.toml`, `gate.lock`, `docs/plan.md`, this task file |

## Goal

Gate's own `gate.toml` uses the core it ships: `[gate] version = "0.3.3"`. Its four plugins, `git`,
`agents`, `markdown` and `ci`, have no release after 0.3.0, so their pins stay. It declares no
`ledger`, so ledger 0.3.2 does not concern it. The shape is that of T-103.

## Proof

- `bin/gate lock`: exit 0, `resolved core 0.3.3`, the four plugins kept; `gate.lock` changes only the
  core's version, URLs and sha256.
- `bin/gate sync`: exit 0, `4 generated, 0 written, 0 removed`, so 0.3.3 writes the same workflow and
  hooks as 0.3.2.
- `bin/gate check`: exit 0, `7 rules, 0 errors, 0 warnings`.

## Resume point

Done.

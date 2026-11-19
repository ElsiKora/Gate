# T-024: Public pass before v0.1.0

| Field | Value |
| --- | --- |
| Status | merged |
| Release | v0.1.0 |
| Writer | "Gate 003: host foundations" |
| Reviewer | the lead reads the diff (documents only) |
| Depends on | none |
| Touches | `docs/architecture.md`, `docs/plan.md`, `docs/tasks/**`, `README.md`, `docs/guide/**`, `spike/RESULTS.md` |

## Goal

The repository goes public from a fresh root at v0.1.0. Its documents describe Gate itself: how a
repository moved to Gate before is not part of it, and every example runs as written against the
current core and plugins.

## Scope

1. Architecture §14: Gate reads only its own configuration files; nothing about any earlier setup
   anywhere else in the architecture.
2. `docs/tasks/**` and `docs/plan.md`: anything about an earlier tool becomes "the reference
   implementation, outside this repository", or goes; D4 says the public history starts at a fresh root
   at v0.1.0.
3. `README.md` and `docs/guide/**` checked against current behaviour: plugin sources, `gate init` and
   `bin/gate`, `gate lock`, `gate sync`, `[ci.github]` with `aggregate`, `[hooks]`, `[steps]`,
   `[suites]`, `[tools]`, exceptions. The lines that declare the git plugin belong to T-019.
4. `spike/RESULTS.md`: no machine path, private name, or number without its command.

## Done when

- `leak-scan.sh` of the local rules prints nothing and exits 0.
- The lead's read of the diff.

## Resume point

2026-11-19T17:53:44+00:00: merged before the public root; its work is in the root commit `59fb163`
and released in `v0.1.0`. Nothing is left.

Earlier:

2026-11-18T21:01:06+00:00: parts 1, 2 and 4; part 3 in the next commit.
README.md and the guides were checked by
reading the code (crates/gate-host/src/config/file.rs, crates/gate/src/main.rs, the SDK, gate-test and each
plugin's rules), not by running the examples. Left to T-019: every line that declares the git plugin or
names a `git/*` rule; those still use the string form and name `git/commit-subject` and
`git/branch-name`, which the git plugin does not declare. The SDK guides depend on `gate-sdk` and
`gate-test` by git at tag `v0.1.0`, which exists from the release on. Waiting: the lead's read.

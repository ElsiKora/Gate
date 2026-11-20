# T-087: Two plugin defaults from the release-candidate trial

| Field | Value |
| --- | --- |
| Status | written; lane round next |
| Release | v0.3.0 (`nestjs` and `docker` plugins) |
| Writer | "Gate 003: host foundations" |
| Reviewer | a separate review session |
| Depends on | `main` at `30a1da26` |
| Touches | `plugins/nestjs/src/rules/structure/barrel_exports.rs`, `plugins/nestjs/tests/barrel_exports.rs`, `plugins/nestjs/rules/barrel-exports.md`, `plugins/docker/src/compose.rs`, `plugins/docker/tests/docker/compose.rs`, `plugins/docker/rules/compose.md`, both plugins' `CHANGELOG.md` |

## Goal

Findings 7 and 8 of the release-candidate trial (T-083):

7. `nestjs/barrel-exports` reported every NestJS service's `src/main.ts` as `#runtime-entrypoint`:
   `private-entrypoints` defaulted to `[]` while `entrypoint-suffix` defaults to `/main.ts`, so
   every project started with an error, and the help did not say that `private-entrypoints` is an
   option of the rule's entry.
8. `docker/compose#port` suggested `"127.0.0.1::5432"` for `"5432:5432"`, which moves a fixed host
   port to a random one; what keeps the behaviour is `"127.0.0.1:5432:5432"`.

## Change

- `private-entrypoints` is optional. Unset, it approves the one entry point NestJS starts from,
  `<layout.source-root>/main.ts` (the source root's file with `entrypoint-suffix`); a list replaces
  the default, and `[]` approves none. The `#runtime-entrypoint` help names the rule option and the
  `[rules]` entry that sets it.
- The `#port` help is built from the entry: the ports after any address (`0.0.0.0:`, `[::]:`)
  behind `127.0.0.1:`, so `<host>:<container>` keeps both, a container port alone keeps its random
  host port (`127.0.0.1::<port>`), and ranges, protocols and `${...}` interpolations with a `:`
  inside are kept. The long form is told to set `host_ip`.

## Tests

- `nestjs`: the source root's `main.ts` passes with the option unset, and with `layout.source-root`
  moved; `[]` and a list without it report it; the help names the rule option.
- `docker`: the help for `5432:5432`, `5432`, a range on both sides, a container range,
  `0.0.0.0:` and `0.0.0.0::`, `[::]:`, `/udp` and an interpolated host port; the suggested entries
  pass; the long form gets `host_ip`.

## Done when

- One lane round on the pushed tip, its own target directory and logs kept per ticket: fmt, clippy,
  test, deny and the release build exit 0; the log shows both plugins compiled from this task's
  round checkout and lists the new tests by name; mutations of each fix fail their test.
- An independent review, one fix round, then the lead decides.

## Resume point

2026-11-20T23:56:33+00:00: both fixes and their tests are written; the lane round is next.

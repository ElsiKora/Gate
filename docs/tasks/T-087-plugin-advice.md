# T-087: Two plugin defaults from the release-candidate trial

| Field | Value |
| --- | --- |
| Status | the review's fix round written; lane round next |
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

## The review of `63efe873`, one fix round

- Blocking: the `nestjs` Bug Fixes entry sat under the released `0.2.1`; it moves under
  `0.3.0 (unreleased)`, and `0.2.1` is byte-identical to `main` again.
- Blocking: `loopback()` split an entry on every `:` while the help splits outside `${...}`, so the
  suggestion for `"${DB_PORT:-5432}:5432"` was itself reported. `loopback()` uses the same split;
  the interpolated suggestions join the assertion that the suggested entries pass.
- An IPv6 address without brackets (`"::1:5432:5432"`) gets the forms themselves, not a suggestion
  built from a misread entry.
- The default of `private-entrypoints` follows `entrypoint-suffix`, which the rule page and the
  changelog now say and a test with `/server.ts` holds.
- The `docker` fix folds into the 0.3.0 Features entry of `docker/compose`, a rule new in 0.3.0.

## Done when

- One lane round on the pushed tip, its own target directory and logs kept per ticket: fmt, clippy,
  test, deny and the release build exit 0; the log shows both plugins compiled from this task's
  round checkout and lists the new tests by name; mutations of each fix fail their test.
- An independent review, one fix round, then the lead decides.

## Resume point

2026-11-21T00:00:33+00:00: the review's fix round is written and pushed; the lane round is next.

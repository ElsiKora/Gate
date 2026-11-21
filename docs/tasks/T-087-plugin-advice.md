# T-087: Two plugin defaults from the release-candidate trial

| Field | Value |
| --- | --- |
| Status | round 406 green after the review's fix round; waits for the lead |
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

## Proof

Lane ticket 399 on `53dc77cc` stopped at clippy (exit 101): `doc_markdown` wanted backticks around
the product name in the doc comment of `private-entrypoints`; the comments now say "the
application" and "a Nest app" (`77913f22`, rebased onto `main` at `6b36eeac`).

Lane ticket 406 on `77913f22`, 2026-11-21T02:37:26+00:00 to 2026-11-21T03:03:09+00:00
(`round-inner.sh`, its own target directory, logs kept under the ticket): fmt 0, clippy 0, test 0
(1506 passed, 0 failed), deny 0 (advisories, bans, licenses, sources ok), release build 0. The test
log shows `Compiling` for every `gate` crate and plugin from the round's own checkout, among them
`gate-plugin-nestjs` and `gate-plugin-docker`, and lists the new tests by name, each `ok`:
`the_source_roots_main_ts_is_approved_when_the_option_is_unset`,
`a_list_replaces_the_default_and_the_help_names_the_rule_option` and
`compose::the_port_help_keeps_the_host_and_container_ports`. Each of the six mutations made its
exact test fail (exit 101):

| Mutation | Test |
| --- | --- |
| the unset option approves nothing | `the_source_roots_main_ts_is_approved_when_the_option_is_unset` |
| the help without "the rule's option" | `a_list_replaces_the_default_and_the_help_names_the_rule_option` |
| a host port dropped (`127.0.0.1::<host>:<container>`) | `compose::the_port_help_keeps_the_host_and_container_ports` |
| the address kept in the suggestion | `compose::the_port_help_keeps_the_host_and_container_ports` |
| `loopback()` splits on every `:` | `compose::the_port_help_keeps_the_host_and_container_ports` |
| the default ignores `entrypoint-suffix` | `the_source_roots_main_ts_is_approved_when_the_option_is_unset` |

## Resume point

2026-11-21T03:03:44+00:00: ticket 406 on `77913f22` is green with every mutation caught; the branch
waits for the lead.

# T-102: `nestjs/raw-config-access` names callback keys and skips getters with no key

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | nestjs 0.3.2 (the next plugin release) |
| Writer | "Gate 011" |
| Reviewer | a session that did not write it |
| Depends on | `main` at `df1a4c5` (release 0.3.2) |
| Touches | `plugins/nestjs/src/rules/structure/raw_config_access/reads.rs`, `plugins/nestjs/tests/raw_config_access.rs`, `plugins/nestjs/rules/raw-config-access.md`, `plugins/nestjs/CHANGELOG.md` |

## Goal

The first consumer's lead asked about three `nestjs/raw-config-access` findings of 0.3.2. The lead
ruled on the rule's meaning, for both shapes in general rather than as special cases:

1. **A key bound by an array-method callback is named**, as the variable of a `for ... of` loop over
   a literal array of strings is named today. The key is the first parameter of an arrow function
   or function expression passed directly as the first argument of `map`, `flatMap`, `forEach`,
   `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `some` or `every`, called (optional
   chaining too) on a literal array of strings or a `const` bound to one, in the same file. The
   parameter is a plain name: not destructured, without a default, not a rest parameter. Not named:
   the first parameter of `reduce` and `reduceRight` (the accumulator), a callback passed by name, a
   later parameter, a method called on anything else.
2. **A getter whose first argument can never be a key is no key read**: neither `dynamic` nor
   coverage. That is, under parentheses, `as`, `satisfies` and `!`, an object literal, an array
   literal, an arrow function or function expression, a class expression, or a numeric, bigint,
   boolean, `null` or regular-expression literal. An identifier, call, member or other expression
   whose value is unknown stays `dynamic`. `boundary` and `process.env` reads do not change.

## Scope

1. `reads.rs`: `bound_keys` follows a parameter to its callback (`callback_keys`) and takes the
   strings of the array the method is called on through `array_keys`; the getter loop skips a call
   whose first argument can never be a key (`never_key`).
2. Pass and fail tests for each named form, each not-named form, and both shapes of ruling 2.
3. The rule page: the Coverage paragraph and the `dynamic` row.
4. A `0.3.2` section in `plugins/nestjs/CHANGELOG.md`. No version changes; the release task bumps
   them.

## Done when

- The tests cover each named and not-named form of ruling 1 and both shapes of ruling 2, each with
  pass and fail cases.
- One lane round on the pushed tip: fmt, clippy `-D warnings`, test and deny exit 0, and the nestjs
  `wasm32-wasip2` component builds.
- The 13 components built at the tip and at `df1a4c5`, sha256 compared: the plugins whose bytes
  change are recorded below (expected: nestjs only).
- On the converted consumer copy of T-098, one untimed cold run without history with this branch's
  nestjs shows exactly five fewer findings than the 85 of 0.3.2 and no new one: the three `dynamic`
  reads of the lead's question and the two `unread` lines the callback reads. The finding lists are
  kept outside the repository.

## Proof

Pending.

## Resume point

2026-11-24T05:00:27+00:00: rulings implemented in `reads.rs`, tests and rule page written; first
lane test job of `raw_config_access` running. Next: commit, push, full lane round, component bytes,
consumer run.

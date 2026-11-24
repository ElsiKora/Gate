# T-102: `nestjs/raw-config-access` names callback keys and skips getters with no key

| Field | Value |
| --- | --- |
| Status | in review |
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

The review of `b60f5f3` found nothing serious; in the fix round the lead ruled further:

3. A loop variable or callback parameter names its keys only if nothing writes it: an assignment, an
   update, or a redeclaration such as a `var` of the same name in the callback. Otherwise the read
   is `dynamic`.
4. An empty array names no key: a read through its loop variable or callback parameter is `dynamic`,
   it does not disappear.
5. `array_keys` sees through `!` as `keys` does; a computed method name (`["K"]["map"]`) stays
   `dynamic`; callback keys in `process.env[key]` are named.

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

Final lane round on `ae6df72` (after the fix round), `one-gate.sh` in Gate's lane, from
2026-11-24T05:48:36+00:00 to 2026-11-24T06:12:04+00:00, lane exit 0, in a detached worktree of the
commit, each workspace crate compiled from it (the log check found none missing). The round before it,
on `31e5637`, stopped at clippy (exit 101, a redundant closure in the write check), fixed in `ae6df72`.

| Command | Exit |
| --- | --- |
| mutant A (no callback keys, no getter skip): `cargo test -p gate-plugin-nestjs --test raw_config_access` | 101: the tests of the named forms, the getter, written names and empty arrays fail |
| mutant B (any parameter of any method's callback names keys): the same | 101: only the test of the not-named forms fails |
| mutant C (a written name still names keys): the same | 101: only the test of written names fails |
| mutant D (an empty array names nothing, silently): the same | 101: only the test of empty arrays fails |
| mutant E (`array_keys` does not see through `!`): the same | 101: only the test of the named forms fails |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --offline --locked --workspace --all-targets --keep-going -j 4 -- -D warnings` | 0 |
| `cargo test --offline --locked --workspace -j 4 --no-fail-fast` | 0 (1636 passed, 0 failed) |
| `cargo deny --offline --locked check` | 0 |
| `scripts/build-component.sh <package> wasm32-wasip2`, for each of the 13 plugins, at `ae6df72` and at `df1a4c5` | 0 each |

The first round, on `c7674ab` before the fix round (2026-11-24T05:05:05+00:00 to 05:36:56+00:00),
exited 0 for the same commands with 1634 tests, and only nestjs bytes changed there too.

The new tests in `plugins/nestjs/tests/raw_config_access.rs`:

- `the_element_parameter_of_a_callback_over_a_literal_array_names_its_keys`: each of the ten methods,
  an arrow function and a function expression, a literal array and a `const` bound to one (also
  `as const` and under `!`), a typed parameter, optional chaining on the member and on the call, a
  parenthesized callback, an inner callback whose parameter shadows the outer one, and callback keys
  in `process.env[key]`; with every key in the example file nothing is reported, without them each
  key is `undeclared` at its line.
- `a_parameter_that_is_no_element_of_a_literal_array_stays_dynamic`: `reduce`, `reduceRight`, `sort`,
  a function declaration and an arrow `const` passed by name, a second parameter, a callback on a
  `string[]` parameter, on a `let` array and on a `const` bound to that parameter, an array with a
  non-string element, a computed method name, a chained receiver, an inner callback over a parameter
  that shadows the outer one, a destructured, a defaulted and a rest parameter, a callback wrapped in
  a call, a callback as the second argument; each read is `dynamic` and each example line `unread`.
- `a_loop_variable_or_callback_parameter_names_keys_only_when_nothing_writes_it`: a `let` loop
  variable and a callback parameter that nothing writes are named (`undeclared` without their lines);
  an assigned loop variable, a loop variable redeclared by `var`, an assigned, an updated and a
  `var`-redeclared callback parameter are `dynamic` and their lines `unread`.
- `an_empty_array_names_no_key_and_its_reads_are_dynamic`: a loop and a callback over `[]`, over a
  `const` bound to `[]` and over `[] as string[]` are `dynamic`; the same over `["K_FULL"]` are named.
- `a_getter_whose_argument_can_never_be_a_key_is_no_read_and_others_stay_dynamic`: an object and an
  array literal, an arrow function, a function and a class expression, numeric, bigint, `true`,
  `false`, `null` and regular-expression literals, and literals under parentheses, `as`, `satisfies`
  and `!` give no finding; an identifier, a member, a call, a conditional, a negated number and a
  `process.env` element are `dynamic`; a store call outside the boundary is still `boundary`, and an
  example line only such a call could read is `unread`.

## Plugin bytes

Every plugin built as released on this machine (macOS host), `shasum -a 256` of each wasm, at
`df1a4c5` (release 0.3.2) and at this branch's tip `ae6df72`, in the final lane round above:

| Plugin | `df1a4c5` | tip `ae6df72` | Changed |
| --- | --- | --- | --- |
| agents | `83c7495484d7aee00b3efe6d71b40bbb834349fd39bc080e9c80d05dabe11098` | same | no |
| cdk | `40dda858e30ac4c6b385cb6e8a11ce7b97ff5f6354a82dfdc41eb09181c1f7be` | same | no |
| ci | `08971b0740b2bf54f0b40b81793d97971cd543ee909bc60bdd6b624492b21169` | same | no |
| docker | `6df6c79b2aa9c8018bd76051e661926f10b92522057bf9fa8324bbc049ef2d70` | same | no |
| docs | `853a35f65145f8d13e10a67aa3052fcef10a7fea88cf6bc094a064e0a948a27b` | same | no |
| frontend | `40941b974730d645292173387f1f17a26fd81d568fccb6f80ca42dda461ee508` | same | no |
| git | `022c92aeccb0e6961e1b20fc4b2da9facdbf983673c4c70c341a63ca0514763a` | same | no |
| ledger | `0517de8eac4461275b587ffca08d9627eecb9fa17a16ddd62bf44dbc1a14ba45` | same | no |
| markdown | `2ee0a7ebbbaaf96fc3205c863f1e673010f83e469fab0d761589a166ae0c755c` | same | no |
| nestjs | `4f692f72b5873ebd655914afaa272c8933f48b4a7d28e19c002e842a06719cb1` | `de94050182549cbe6315f0cc25dc2b9e2408d94378b1a67583f60fb1109710f7` | yes |
| node | `444b900da4e2f658221cb9c6d98d17708b689c7d9564d7e09142e4312f465e09` | same | no |
| repository | `b11c7aada6c9dc8aa3c53343081fdbc1fdf365e9336b27764a87066caea65b5f` | same | no |
| typescript | `3ee4b47b10555ceffdaad0c961e7168f6248bd2ad0e4601ee56e3ce5d87c5351` | same | no |

Only nestjs changes. The `df1a4c5` column equals the tip column of T-101's table for every plugin.

## Consumer run

On the converted consumer copy of T-098, locked on the released 0.3.2 core and plugins, one untimed
cold check without history each (`gate check --format json --skip-rule git/main-history --skip-rule
git/no-ci-skip`, from the pinned blobs alone): with the released nestjs 0.3.1, 85 findings
(2026-11-24T05:02:37+00:00, exit 1; the same list T-098 recorded); with the tip's nestjs pinned by
`gate lock --set ... --file`, 80 findings (the nestjs of `c7674ab`, 2026-11-24T05:37:40+00:00,
exit 1). Five removed, none
added: the three `dynamic` reads of the lead's question and the two `unread` example lines the
callback reads. After the fix round, the same run with the nestjs of `ae6df72` (2026-11-24T06:12:44+00:00,
exit 1) gives the same 80 findings, list for list. The lists and the diffs are kept outside the
repository.

## Resume point

2026-11-24T06:13:02+00:00: fix round done. Gates green on `ae6df72`, only nestjs changes, the consumer
run still shows the five findings cleared and none added. Next: the lead decides.

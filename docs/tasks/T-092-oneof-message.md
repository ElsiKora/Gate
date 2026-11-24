# T-092: Rule options that fail a `oneOf` say what is wrong and what to write

| Field | Value |
| --- | --- |
| Status | in review |
| Release | the next core release |
| Writer | the T-092 writer |
| Reviewer | a session that did not write it |
| Depends on | `main` at `d8f213a` |
| Touches | `crates/gate-host/src/plugins.rs` (`JsonSchemaValidator`), its unit tests, `crates/gate/tests/plugins.rs`; the lead writes the CHANGELOG entry at the release |

## Goal

A rule option, or a plugin option, that breaks its JSON Schema printed the validator's own text for
the combining keywords. With the released 0.3.0 core and `"git/branch-name"`:

| Given `options` | Printed after `"git/branch-name" options:` |
| --- | --- |
| `{}` | `{} is not valid under any of the schemas listed in the 'oneOf' keyword` |
| `{ pattern = "x", template = ":type/:name" }` | `{"pattern":"x","template":":type/:name"} is valid under more than one of the schemas listed in the 'oneOf' keyword` |
| `{ pattern = "x", subject = true }` | `/subject: true is not valid under any of the schemas listed in the 'anyOf' keyword` |
| `{ pattern = "x", subject = { name = 3 } }` | `/subject: {"name":3} is not valid under any of the schemas listed in the 'anyOf' keyword` |
| `{ pattern = "x", bogus = 1 }` | `Additional properties are not allowed ('bogus' was unexpected)` |

T-085 fixed the same class for `enum`: `/pack: "next" is not one of "react-vite", "nextjs" or
"react-library"`. This task does the same for every combining keyword and the other raw wordings of
the validator.

## Design

One translator, `JsonSchemaValidator::validate`, turns each validator error into one line. The
caller already names the rule (`"<plugin>/<rule>" options:`) or the plugin (`plugin <name>
options:`) and the file and line; the line starts with the option path (`/subject: `), left out
for the options table itself, as before. There is no other message path: every kind below is
written by the translator, never by the validator's `Display`.

**Forms.** A schema is named as a list of forms, through local `$ref`s:
- a `const` or an `enum` value: the value, as JSON (`"release"`, `3`);
- a `type`: `a string`, `an integer`, `a number`, `true or false`, `an array`, `a table`; `null` is
  left out, since `gate.toml` cannot write it;
- a table names its `required` keys and the `const` of its properties: `a table with kind =
  "release" and subject`;
- a nested `oneOf` or `anyOf` without a `type` of its own: the forms of its branches.

Anything else (`allOf`, `not`, an empty schema, a branch with neither `type` nor `required`) cannot
be named, and the whole keyword falls to its generic message.

**The value given** is its JSON; a table or array longer than 60 characters is written `a table of
a, b and c` or `an array of 4 items`.

| Validator error | Message |
| --- | --- |
| `oneOf`, none fits, every branch only `required` keys | `sets none of template or pattern; set exactly one of them` |
| `oneOf`, more than one fits, every branch only `required` keys, two or more set | `sets template and pattern; set exactly one of template or pattern` |
| the same, the value not a table (every such branch passes it) | `"x" is not a table` (one line with the `type` error's) |
| the same, fewer than two keys set | the generic `fits more than one of the forms` line |
| `anyOf`, none fits, every branch only `required` keys | `sets none of a or b; set at least one of them` |
| `oneOf` or `anyOf`, every form a constant (T-085) | `"next" is not one of "react-vite", "nextjs" or "react-library"` |
| `oneOf` or `anyOf`, the value has the shape of one or more branches | the nearest branch's own errors (fewest errors, the first on a tie), each translated at its own path |
| `oneOf` or `anyOf`, none fits, forms named | `true is not a string or a table`, leaving out the forms the value has |
| `oneOf`, more than one fits, forms named | `{given} fits more than one of the forms a, b or c; change it so it fits exactly one` |
| `not` of `required` keys | `sets k, which is not allowed here; remove it` |
| `not`, form named | `{given} must not be {form}` |
| `const` | `{given} is not {value}, the only value allowed here` |
| `enum` (T-085) | `{given} is not one of a, b or c`; an `enum` of only `null`: `{given} is not null, the only value allowed here` |
| `type` | `"three" is not an integer` |
| `required` | `kind is required; set it` |
| `additionalProperties` | `unknown key bogus; the keys are a, b and c` (`and keys matching ^x-` for `patternProperties`, whose keys the validator never reports; `this table takes no keys` when it has none) |
| `false` schema | `{given} is not allowed here` |
| `additionalProperties: false` beside no `properties` (the validator reports it as a `false` schema at the table, with its first value: 0.3.0 printed `False schema does not allow 1` for `[plugins.agents] options = { x = 1 }`) | `unknown key x; this table takes no keys` |
| `contains`, form named | `{given} holds no item that is {form}` |
| an error inside `then` or `else` | its own message, then `, because {path} is {if form}` (`is not` for `else`) when the `if` can be named and the value has (or lacks) that form; a table `if` only when it tests nothing but the `const` of keys, each required in the `if` or beside it |

A value "has the shape of a branch" when that branch reports no `type`, `const`, `enum`, `not`,
`false` or `required` error at the value itself and no `const` error at a key of it (a tag). The
error is then inside that form, and the form's own message is the useful one: `{ name = 3 }` for
`subject` is a table of strings with a number in it, `/subject/name: 3 is not a string`. When more
than one branch has the shape, the nearest speaks: the one with the fewest errors, the first on a
tie.

**Keywords are read by position.** The error's canonical schema location is walked from the root;
a segment is a keyword only where the walk is at a schema object, so a property named `then`,
`else`, `if` or `additionalProperties` is a property. A `then` in a definition keeps its clause;
an error the `then` reaches through a `$ref` has a canonical location in the referenced schema,
which does not show the `then`, and gets no clause.

**One budget per error.** Naming the forms of one error visits at most 256 schema nodes, each
`$ref` followed counting one; past it, the generic message. Two errors that say the same are one
line.

**Generic messages** (a case the translator cannot name plainly); each still follows the rule and
the option path:
- `oneOf` or `anyOf`, none fits, a form not nameable: `{given} fits none of the forms this option
  allows`;
- `oneOf`, more than one fits, a form not nameable: `{given} fits more than one of the forms this
  option allows; it must fit exactly one`;
- `not`, its schema not nameable: `{given} is not allowed here`;
- `contains`, the item schema not nameable: `{given} holds no item of the form this option needs`.

**Kept in the validator's words**, which already say what is wrong in plain terms and come after the
option path: `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`,
`minLength`, `maxLength`, `pattern`, `format`, `minItems`, `maxItems`, `uniqueItems`,
`minProperties`, `maxProperties`, `additionalItems`, `unevaluatedItems`, `unevaluatedProperties`,
`propertyNames`, the content and regex-engine errors.

## Done when

- Unit tests of `JsonSchemaValidator` cover each translated kind, pass and fail, each asserting
  the new text and that the validator's raw text is gone.
- A CLI test with the built git plugin: `"git/branch-name"` with `options = {}` names template and
  pattern.
- One lane round on the pushed tip exits 0 for fmt, clippy `-D warnings`, test, deny and the release
  build.
- No plugin component bytes change: this touches the host only.

## Proof

Lane round on `37f2d39`, `one-gate.sh` in Gate's lane, from 2026-11-24T19:04:23+00:00 to
2026-11-24T19:24:07+00:00 (1184 s), lane exit 0, in a detached worktree of the commit with hooks off;
the log check found every workspace crate compiled from it:

| Command | Exit |
| --- | --- |
| mutant: `explain` replaced by the validator's own text, `cargo test -p gate-host --lib` | 101 (all 10 validator tests fail, as they must) |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --offline --locked --workspace --all-targets --keep-going -j 4 -- -D warnings` | 0 |
| `cargo test --offline --locked --workspace -j 4 --no-fail-fast` | 0 (1645 passed, 0 failed) |
| `cargo deny --offline --locked check` | 0 (advisories, bans, licenses, sources ok) |
| `cargo build --release --offline --locked -j 4` | 0 |

The 13 components, each built by `scripts/build-component.sh <package> wasm32-wasip2` at `37f2d39`
and at `main` `d8f213a` on the same target directory: every sha256 is the same at both (git
`022c92ae…`, nestjs `aa5715bd…`, the others likewise). No plugin component bytes change; the change
is in the host only.

`leak-scan.sh` on the worktree and with `--branch origin/main`: no output, exit 0.

## Resume point

2026-11-24T19:24:18+00:00: the lane round on `37f2d39` exited 0 (see Proof). Next: the lead's
review.

2026-11-24T19:05:00+00:00: the translator and its tests are written (`37f2d39`); a lane job on
`gate-host` alone ran clippy `-D warnings` (exit 0) and the 14 validator tests (14 passed). Next: the
lane round on `37f2d39`, then the report to the lead.

2026-11-24T18:53:20+00:00: design written; next, the translator and its tests.

# T-041: The error-logging page says what the rule accepts

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 004: public presentation" |
| Reviewer | the lead reads the page |
| Depends on | `release/v0.2.0` |
| Touches | `plugins/nestjs/rules/error-logging.md` |

## Goal

A reader of the `nestjs/error-logging` page can tell, before running the rule, whether a native
error log passes `#logger-error-evidence`. The page says what the code does; the code does not
change.

## Scope

1. "Actual error evidence" is defined: a `catch` binding at any depth, a parameter typed `unknown`
   or the library `Error`, or a `const` alias of one.
2. The argument order: `(evidence, message)` for a call on the class, `(message, evidence)` for a
   call on an instance.
3. The message may be any template literal that reads no raw evidence; reading follows `const`
   initializers and the initializers of the file's class members.
4. One example of a rejection callback that passes: its parameter typed `unknown`.

Not in scope: counting an untyped rejection callback parameter as evidence. That would change
findings against the reference implementation and is a decision of its own.

## Done when

- The page states items 1 to 4 in the house style, with synthetic names only.
- The leak scan named in the local rules prints nothing.
- The lead has read the page.

## Resume point

2026-11-19: task file written. Next: the page.

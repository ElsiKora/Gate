# T-115: `nestjs/typeorm-lock-owner`, one owner for TypeORM row locks

| Field | Value |
| --- | --- |
| Status | open (requested by the first consumer for two products; no date asked) |
| Release | the next `nestjs` plugin release, with T-114 |
| Writer | assigned (branch `task/T-115-nestjs-typeorm-lock-owner`) |
| Reviewer | a session that did not write it; the lead decides after one fix round |
| Depends on | `main` at `faf776e` |
| Touches | `plugins/nestjs/` (a new rule under `src/rules/semantics/typeorm_lock_owner/`, its page, `src/lib.rs`, `CHANGELOG.md`, tests), `docs/plan.md`, this task file |

## Goal

A consumer asked for one shared rule: every TypeORM read that takes a row lock gets its lock from
one owner function, and only through recognised read shapes. The specification is kept outside the
repository; the lead gives its path in the brief. It is input, not a contract: this task file
overrides it wherever they differ.

## Decisions (the lead's)

1. **Home and id.** A new rule, `nestjs/typeorm-lock-owner`, project scope, TypeScript only, in no
   preset (it needs the project's `owner` option). `advisory-lock-boundary` and `typeorm-relation`
   do not fit: the first reports per file by property name, the second checks entity declarations.
2. **Aspects.** `owner`, `unreadable`, `receiver`, `builder`, `lock-object`, `lock-value` and
   `row-list`. The last three are the specification's `object`, `option` and `guarded`.
3. **Options:** `owner`, `roots`, `framework-package` (defaults to `layout.automator-package`),
   `service-base`, `service-read-methods`, `step-context-method`, and two new ones:
   - `read-methods`: TypeORM's find-option methods by default (`count`, `exists`, `find`,
     `findAndCount`, `findOne`, `findOneOrFail`). The `*By` methods take a `where`, not find
     options, so a `lock` key there is a column filter and they are left out (the lead's answer to
     the design, 2026-11-27). A call's shape alone does not make it a read.
   - `lock-modes`: the four row-lock modes by default. `optimistic` and `dirty_read` are not row
     locks: a `lock` whose mode folds to one of them is no site. A mode that folds to any other
     string outside `lock-modes`, or that does not fold, is a `lock-value` finding (the lead's
     answer to the design, 2026-11-27).

   The service base is matched by an import from `framework-package`, as `native-repository` does,
   never by name alone.
4. **Resolution.** Through the project's module resolution, the shared `Sources` and the tsconfig
   resolver, not `layout.path-aliases`. Scope binding comes first, so that a shadowed local name is
   never taken for the owner.
5. **Lock values.** Literals, `const` bindings and imports, which `ValueReader` already folds. Add
   enum members, and the declared-type reach from syntax alone: symbol, declaration, its written
   type annotation, the enum's member values. Explicit annotations only.
6. **Which `lock` keys are read.** Only in an object that is an argument of a recognised read
   (also through conditionals and spreads), plus any `lock` whose value is an owner call. A `lock`
   key in any other object is not checked.
7. **Another service's entity.** Recognised when a constructor parameter property or a typed class
   property names a class that directly extends `service-base<E>` of `framework-package`.
   Otherwise only the shape is checked.
8. **Findings.** No target, per the plugin's norm. A `key` from the enclosing `Class.method` or
   function plus an ordinal. One finding per lock site, where a site is the `lock` property or the
   `setLock` call, whatever lines it spans. Options may be inline or in an options file; with inline
   options an `owner` finding is placed at the configuration.
9. **Ambiguities, settled:**
   - "Exports exactly one function": one value export that is a function declaration or a `const`
     arrow function, default or named. Type exports do not count.
   - `this.getMany(...)` in a class that does not directly extend `service-base`: not a service
     read, so no aspect applies.
   - "After its joins": dropped, since no aspect checks it.
   - A guarded list need not name the builder's own receiver.
   - An entity-manager read whose entity is not an identifier: `receiver` reports it as
     unrecognised.
10. **The interface stays `gate:plugin@0.3.0`.** If the work needs a change to it, stop and report
    to the lead before writing code.

## First step: the design, to the lead

Before writing code, send the lead a short design. It covers the read classifier, the enum and
declared-type reach, how receivers are compared (by syntax tree, not by token), and the known false
negatives the page will list.

## Proof

- Every fixture of the specification is rewritten as a synthetic tree. Use neutral names (for
  example `src/lock/row-lock.ts`, `lockRows`, `RowLockStrength`), not a consumer's file suffixes or
  prefixes. The public package API names the plugin already defaults to may stay.
- Each aspect has its pass and fail. The false positives the design names are each tested to
  pass: `setLock` on another object, `mode` in an unrelated object, `this.get` outside a service.
- No product, folder or norm id of any consumer appears in code, tests, pages, the CHANGELOG or
  commits.
- The comparison with the consumer's reference figures runs outside the repository. The writer
  reports it to the lead and does not commit it.
- All cargo runs go through the lane.

## Resume point

The design is accepted (2026-11-27; note `T-115-design.md` in the lead's notes folder). The rule, its
page, the tests and the CHANGELOG entry are written; next: the gates through the lane, then the
comparison with the specification's figures (outside the repository), then the report to the lead.

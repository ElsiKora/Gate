# T-034: Stable finding fingerprints and a baseline that only shrinks

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | T-029 at `b1173fb` (the interface at `gate:plugin@0.2.0`) |
| Touches | `wit/gate.wit` (the finding record), `crates/gate-sdk` (the finding builder), `crates/gate-runtime` (the finding conversion, the probe), `crates/gate-host` (fingerprint, baseline, reports), `crates/gate` (`gate baseline prune`), the seven plugins (finding construction lines and their collectors only), docs, changelogs |

## Goal

A plugin can reword a finding's message without making every repository's baseline entry for it
stale, and a repository can shrink its baseline after fixes without accepting a new finding.

Measured by a consumer on the reference tree (lead, 2026-11-19): T-031 makes 54 of 349 baseline
entries stale. 2 are option-file paths that change. The other 52 are
`nestjs/field-inventory#unclassified` messages that embed the option file's path. A fingerprint is
(rule, aspect, path, message digest), so any wording change breaks every consumer's baseline. And
`gate baseline write` adds new findings silently, so consumers check by hand that it added nothing.

## Scope

1. WIT, on top of T-029's `gate:plugin@0.2.0` (no second bump): a finding gets an optional `key`, a
   short stable identity of what it is about within its rule, aspect and path (`ProviderEntity.name`,
   a controller class name). The interface says it must not change when the message's wording does.
2. The SDK's finding builder gets `.key(..)`.
3. The host fingerprint is (rule, aspect, path, key) when a key is given, otherwise (rule, aspect,
   path, message digest) as before. A keyed digest adds the word `key` before the key, so a key never
   gives the fingerprint of a message of the same text. A baseline entry stores the key and checks its
   fingerprint against it. SARIF keeps `partialFingerprints."gate/v1"`, whose value is that
   fingerprint: unkeyed findings keep theirs. `docs/architecture.md` section 6 says so.
4. Keys in the plugins for every finding whose message embeds a line, a column, a path or an option
   value (lead, 2026-11-19: on the reference tree 40 of 363 baseline entries carry `file.ts:<line>`
   in their message, so an unrelated edit above them fails the check; 9 moved between two commits).
   A key names what the finding is about (field, class, symbol, specifier, module, record id, the
   check after `#` where one subject can fail several) and never holds a line or a column. Only the
   lines that build findings, and the plugins' own collectors (`Findings::push_keyed` in nestjs,
   `Findings::emit_keyed` in ledger), change; Gate 006 works in `plugins/nestjs` for T-030.
5. `gate baseline prune`: a full run of the working tree, then the removal of the entries `gate check`
   reports as stale (no finding matches them, and their rule ran in their scope). It prints each
   entry it removed and how many findings stay unaccepted, never adds an entry, and rewrites the file
   only when it removed one. `gate baseline write` stays for the first baseline, and its help says
   that later fixes use prune.
6. Tests: a message change with a key keeps the fingerprint (JSON and SARIF) and the baseline entry;
   without a key it changes and the entry is stale; prune removes only stale entries, adds none,
   leaves a new finding reported, exits 0, and exits 2 on a baseline edited by hand.

### Rules keyed in this task

- nestjs, structure: `adapter-boundary`, `advisory-lock-boundary`, `advisory-lock-manifest`,
  `asset-storage`, `automator-metadata`, `barrel-exports`, `constant-files`, `controller-owners`,
  `declaration-files`, `dto-decorators`, `enum-values`, `error-logging`, `field-inventory`,
  `import-direction`, `notification-egress`, `package-pins`, `raw-config-access`, `runtime-factory`,
  `string-enums` (`error-string-shim`).
- nestjs, semantics: `api-readiness`, `custom-routes`, `entity-descriptions`, `entity-table-name`,
  `import-aliases`, `kind-layout` (`utility-placement`, `unit-namespace`), `migrations` (`baseline`,
  `registry`), `module-names`, `module-registry`, `native-functions`, `native-repository`,
  `owner-boundaries` (`owner-barrel`, `entity-manager-exposure`, `domain-module-forwarding`,
  `api-plane-route` internal base), `prefix-nesting`, `typeorm-relation`, `wrappers`.
- agents: `rule-ids`, `rule-registry`, `skills`, `instruction-references`, `rule-enforcement`,
  `adapters`, `rule-file-size`.
- ci: `workflow-run-target`. git: `branch-name`, `commit-message`, `main-history`.
- markdown: `frontmatter`, `local-links`, `required-sections`. typescript: `parse`.
- ledger: records (`record-counterpart`, `record-field`, `parity-fields`, `record-shape`,
  `record-unique`, `record-commit`, `record-closure`, `record-dependency`, `dependency-cycle`),
  `record-status`, decisions, manifest, reuses, `closing-notes`, blockers, `exception-owners`,
  `exception-decisions`, the review-progress generator's count problems.

Where several findings of one check in one file have no finer subject at the line that builds them
(nestjs `error-logging`, some `constant-files` shapes, the `advisory-lock-manifest` malformed marker,
the grammar errors `typeorm-relation` passes through per member), they share a key. Their messages
differed only by the line before; the baseline accepts one finding per entry, so they still count.
A finer key there (the enclosing function, say) needs more than the finding lines and is part of the
follow-up.

### Follow-up after v0.2.0 (lead, 2026-11-19)

Findings left unkeyed because their message is fixed text or names only facts that already identify
them; a key would change nothing today, but one makes later rewording safe:

- nestjs: `eslint-directives`, `owner-roots`, `repeated-segment`, the path-only `kind-layout`,
  `module-names` (`shortened-resource-folder`) and `owner-boundaries` findings, `migrations`
  (`filename`, `timestamp`, `class`, `barrel`), `string-enums` (`literal-union`,
  `test-literal-union`, `keyof-typeof-alias`, `raw-discriminant`, `unsupported-namespace`,
  `raw-enum-value`), `advisory-lock-manifest` manifest and marker messages, `advisory-lock-boundary`
  (`unreadable`, `migration`), `field-inventory#unreadable`.
- agents: `no-claude-md`, `rule-owner`, `rule-references`, `agents-block`, the fixed-text
  `rule-registry` and `skills` findings. ci: `actions-pinned`. git: `no-ci-skip`.
- ledger: the fixed-text record, decision and catalog findings and the managed-block finding.
- The shared keys above, with a finer subject.

## Tests

- `crates/gate/tests/plugins.rs`:
  - `a_finding_with_a_key_keeps_its_fingerprint_when_its_message_changes`: the probe's `read` rule
    reports at `a.txt` with a message and key from its options. The JSON fingerprint and SARIF's
    `partialFingerprints` stay when the message changes under one key, and change with another key.
    Without a key they change with the message. A key never equals the fingerprint of the same text
    as a message.
  - `a_baseline_entry_with_a_key_accepts_its_finding_after_the_message_changes`: with a key the
    reworded finding stays accepted; without one it is reported and its entry is stale.
  - `a_finding_that_names_its_line_keeps_its_fingerprint_when_an_edit_above_moves_it`: the probe
    reports `x.ts:2 class Thing` keyed by what it found; a comment added above moves it to
    `x.ts:4`, and its fingerprint and its baseline entry stay.
  - `baseline_prune_removes_only_stale_entries_and_never_adds_one`: one finding fixed and one new.
    Prune removes the fixed one's entry, prints it, keeps the other, adds nothing, and the new finding
    stays reported. A second prune changes nothing. A baseline edited by hand exits 2 and is not
    written.
- `crates/gate-host/tests/host/findings.rs`:
  - `pruning_drops_exactly_the_entries_check_reports_stale`: of two equal entries one stays, an entry
    of a rule that did not run stays, a fixed finding's entry goes, and the pruned baseline leaves
    only the new finding reported.
  - `a_key_replaces_the_message_in_the_fingerprint`.

## Done when

- The tests above pass; `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets
  -- -D warnings`, `cargo test --locked --workspace`, `cargo deny --locked check` and
  `cargo build --release --locked` exit 0 in the lane on the pushed commit, and the leak scan prints
  nothing.
- Do not edit docs/plan.md; the lead keeps it.

## Resume point

2026-11-19: branch `task/T-034-stable-fingerprints` from T-029's `b1173fb`; the lead rebases it onto
the release branch. Interface, SDK, host, prune, keys in all seven plugins, tests and docs written;
round 167 was withdrawn before it ran when the scope grew; next: one lane round on the final tip,
then the report.

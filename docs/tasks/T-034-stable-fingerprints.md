# T-034: Stable finding fingerprints and a baseline that only shrinks

| Field | Value |
| --- | --- |
| Status | in progress |
| Release | v0.2.0 |
| Writer | "Gate 001: the gate as a standalone tool" |
| Reviewer | a separate review session |
| Depends on | T-029 at `b1173fb` (the interface at `gate:plugin@0.2.0`) |
| Touches | `wit/gate.wit` (the finding record), `crates/gate-sdk` (the finding builder), `crates/gate-runtime` (the finding conversion, the probe), `crates/gate-host` (fingerprint, baseline, reports), `crates/gate` (`gate baseline prune`), `plugins/nestjs` (finding construction lines only), docs, changelogs |

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
4. Keys in the plugins for every aspect whose message embeds a path or an option value. This task:
   `nestjs/field-inventory` (every such aspect) and `nestjs/controller-owners` (every aspect), with a
   `#<check>` after the subject where one subject can fail several checks of one aspect. Gate 006
   works in `plugins/nestjs` for T-030 (speed, no messages); this task changes only the lines that
   build findings, plus `Findings::push_keyed` in `plugins/nestjs/src/shared/report.rs`.
5. `gate baseline prune`: a full run of the working tree, then the removal of the entries `gate check`
   reports as stale (no finding matches them, and their rule ran in their scope). It prints each
   entry it removed and how many findings stay unaccepted, never adds an entry, and rewrites the file
   only when it removed one. `gate baseline write` stays for the first baseline, and its help says
   that later fixes use prune.
6. Tests: a message change with a key keeps the fingerprint (JSON and SARIF) and the baseline entry;
   without a key it changes and the entry is stale; prune removes only stale entries, adds none,
   leaves a new finding reported, exits 0, and exits 2 on a baseline edited by hand.

### Keys left for later tasks

The survey of `plugins/` found these further findings whose messages embed a path or an option value
(file and line at `b1173fb`). Each needs a key. They are outside this task, so the nestjs merge with
T-030 stays mechanical.

- `nestjs/notification-egress`: `unclassified-package` (scan.rs:74), `unclassified-egress`
  (scan.rs:127), `direct-notification` (scan.rs:176), `stale-owner` (scan.rs:207, 224, 259, 293,
  312, 337).
- `nestjs/advisory-lock-boundary`: `inventory` (mod.rs:447), `boundary` (mod.rs:341), the
  rejections (mod.rs:324).
- `nestjs/automator-metadata`: `policy-key` (mod.rs:322), `dto-decorator`, `entity-description` and
  `route-contract` (mod.rs:218), `controller-decorator` (mod.rs:95).
- Other nestjs rules whose message starts with the finding's own path or `path:line`, or names an
  option: `custom-routes`, `entity-table-name`, `import-aliases`, `kind-layout`, `module-names`,
  `native-repository`, `owner-roots`, `prefix-nesting`, `repeated-segment`, `api-readiness`,
  `native-functions`, `owner-boundaries`, `typeorm-relation`, `wrappers`, `migrations`,
  `module-registry`, `package-pins`, `advisory-lock-manifest`.
- `agents`: `rule-ids`, `rule-registry`, `skills`, `instruction-references`, `rule-enforcement`,
  `adapters`, `rule-file-size`.
- `git`: `main-history`, `branch-name`, `commit-message`.
- `markdown`: `frontmatter`, `local-links`, `required-sections`.
- `typescript/parse`.
- `ledger`: the record, decision, manifest, reuse, closing-note and blocker findings
  (`catalog.rs:86`), keyed by the record id.

## Tests

- `crates/gate/tests/plugins.rs`:
  - `a_finding_with_a_key_keeps_its_fingerprint_when_its_message_changes`: the probe's `read` rule
    reports at `a.txt` with a message and key from its options. The JSON fingerprint and SARIF's
    `partialFingerprints` stay when the message changes under one key, and change with another key.
    Without a key they change with the message. A key never equals the fingerprint of the same text
    as a message.
  - `a_baseline_entry_with_a_key_accepts_its_finding_after_the_message_changes`: with a key the
    reworded finding stays accepted; without one it is reported and its entry is stale.
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
the release branch. Interface, SDK, host, prune, the two nestjs rules, tests and docs written; next:
the lane round, then the report.

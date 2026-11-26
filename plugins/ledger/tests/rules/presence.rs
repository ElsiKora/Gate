//! A ledger rule that cannot read its input fails: an absent ledger reports every file a rule
//! reads, a partial one reports what is missing, and a complete one at the plugin's defaults
//! passes every rule.

use crate::{
    Files, IMPLEMENTATION, PARITY, PLAN, Seen, findings, findings_in, input, ledger, seen,
};

const MANIFEST: &str = "docs/ledger/manifest.json";
const INDEX: &str = "docs/ledger/decision-index.md";
const COVERAGE: &str = "docs/adr/decision-coverage.md";
const CATALOG: &str = "docs/reference/mechanisms.md";

/// The synthetic manifest of the task file: the plugin's five default files and three families.
const SYNTHETIC: &str = r#"{
  "canonicalRoot": "docs/ledger",
  "canonicalFiles": ["decision-index.md", "implementation-slices.md", "verification.md",
                     "follow-ups.md", "production-blockers.md"],
  "idFamilies": {
    "D": { "pattern": "`D-[A-Z-]+-\\d{3}`", "file": "decision-index.md" },
    "I": { "pattern": "`I-[A-Z-]+-\\d{3}`", "file": "implementation-slices.md", "unique": true },
    "V": { "pattern": "`V-[A-Z-]+-\\d{3}`", "file": "verification.md", "unique": true }
  },
  "forbiddenReferences": []
}"#;

/// Every rule of the plugin, with the options it requires.
const RULES: [(&str, &str); 10] = [
    ("blockers", ""),
    ("closing-notes", r#"{"field":"`TEMPORARY_DELTA` records"}"#),
    ("config-decisions", ""),
    ("decisions", ""),
    ("exception-owners", ""),
    ("manifest", ""),
    ("record-status", ""),
    ("records", ""),
    ("reuses", r#"{"catalog":"docs/reference/mechanisms.md"}"#),
    ("root-exception-owners", ""),
];

/// The findings of every rule over `files`, with [`PARITY`], by rule.
fn every_rule(files: &Files) -> Vec<(&'static str, Vec<Seen>)> {
    RULES
        .iter()
        .map(|(rule, options)| (*rule, findings(rule, options, PARITY, files)))
        .collect()
}

/// A complete ledger at the plugin's defaults: the record fixture, the manifest, the decision
/// index with its record, the ADR coverage with its ADR, and the catalog of shared mechanisms.
fn complete() -> Files {
    ledger()
        .with(MANIFEST, SYNTHETIC)
        .with(
            INDEX,
            "# Decisions\n\
             \n\
             | ID | Status | Title | File |\n\
             | --- | --- | --- | --- |\n\
             | `D-ONE-001` | `accepted` | One | `decisions/one.md` |\n",
        )
        .with("docs/ledger/decisions/one.md", "- `D-ONE-001`: one\n")
        .with(
            COVERAGE,
            "# Coverage\n\
             \n\
             | `D-ONE-001` | `accepted` | `0001-one.md` | `decisions/one.md` |\n",
        )
        .with(
            "docs/adr/0001-one.md",
            "# 0001 One\n\
             \n\
             - Linked ledger IDs: `D-ONE-001`\n\
             \n\
             ## Context\n\
             ## Decision\n\
             ## Consequences\n\
             ## Alternatives Considered\n\
             ## Verification\n",
        )
        .with("docs/adr/0000-template.md", "# Template\n")
        .with(
            CATALOG,
            "# Shared mechanisms\n\
             \n\
             - `shared-parser`: the parser\n\
             \x20 - Decisions: `D-ONE-001`\n\
             \x20 - Path: `src/parser.ts`\n",
        )
        .with("src/parser.ts", "export {};\n")
}

#[test]
fn an_absent_ledger_fails_every_rule_that_reads_a_file_naming_it() {
    let implementation = |rule| {
        input(
            rule,
            IMPLEMENTATION,
            "[plugins.ledger] options implementation",
        )
    };
    let plan = |rule| input(rule, PLAN, "[plugins.ledger] options parity.plan");
    let records = |rule| vec![implementation(rule), plan(rule)];
    assert_eq!(
        every_rule(&Files::default()),
        [
            ("blockers", records("blockers")),
            ("closing-notes", records("closing-notes")),
            ("config-decisions", vec![]),
            (
                "decisions",
                vec![
                    input("decisions", INDEX, "[plugins.ledger] options decisions"),
                    input("decisions", COVERAGE, "its option coverage"),
                ]
            ),
            ("exception-owners", vec![]),
            (
                "manifest",
                vec![seen(
                    "input",
                    MANIFEST,
                    None,
                    "docs/ledger/manifest.json is not tracked, and ledger/manifest reads it (its option manifest); create it, point the option at the file, or turn the input off with a decision: \"ledger/manifest\" = { levels = { input = \"off\" }, decision = \"<record>\" }"
                )]
            ),
            ("record-status", records("record-status")),
            ("records", records("records")),
            (
                "reuses",
                vec![
                    implementation("reuses"),
                    plan("reuses"),
                    input("reuses", CATALOG, "its option catalog"),
                    input("reuses", INDEX, "[plugins.ledger] options decisions"),
                ]
            ),
            ("root-exception-owners", vec![]),
        ]
    );
}

#[test]
fn a_complete_ledger_at_the_defaults_passes_every_rule() {
    for (rule, found) in every_rule(&complete()) {
        assert_eq!(found, [], "{rule}");
    }
}

#[test]
fn a_partial_ledger_reports_what_is_missing() {
    // The manifest and the index are tracked; the implementation file and the coverage are not.
    let files = complete().without(IMPLEMENTATION).without(COVERAGE);
    let mut manifest = findings("manifest", "", PARITY, &files);
    manifest.sort();
    assert_eq!(
        manifest,
        [
            seen(
                "manifest-file",
                IMPLEMENTATION,
                None,
                "docs/ledger/implementation-slices.md is required by the I-family definition"
            ),
            seen(
                "manifest-file",
                IMPLEMENTATION,
                None,
                "docs/ledger/implementation-slices.md is required by the ledger manifest"
            ),
        ]
    );
    assert_eq!(
        findings("records", "", PARITY, &files),
        [input(
            "records",
            IMPLEMENTATION,
            "[plugins.ledger] options implementation"
        )]
    );
    assert_eq!(
        findings("decisions", "", PARITY, &files),
        [input("decisions", COVERAGE, "its option coverage")]
    );

    // Every input is tracked, and the manifest lists one more file: only the manifest reports it.
    let files = complete().edit(
        MANIFEST,
        "\"production-blockers.md\"]",
        "\"production-blockers.md\", \"glossary.md\"]",
    );
    for (rule, found) in every_rule(&files) {
        let expected = if rule == "manifest" {
            vec![seen(
                "manifest-file",
                "docs/ledger/glossary.md",
                None,
                "docs/ledger/glossary.md is required by the ledger manifest",
            )]
        } else {
            vec![]
        };
        assert_eq!(found, expected, "{rule}");
    }
}

#[test]
fn the_root_rule_in_its_own_call_names_the_file_it_read() {
    // The repository call of a root that is not a project, with no ledger but a root exception
    // that names an owner: the root rule reports what its project rule reports, naming the
    // implementation file it read.
    let files = Files::default().with(
        "gate.exceptions.toml",
        "[[exception]]\nrule = \"demo/rule\"\ntarget = \"README.md\"\nreason = \"kept for now\"\nowner = \"I-NOPE-001\"\ndecision = \"D-NOPE-001\"\n",
    );
    for (project, root, read) in [("exception-owners", "root-exception-owners", IMPLEMENTATION)] {
        let found = findings_in(root, "", PARITY, "gate.toml", &files);
        assert_eq!(found, findings(project, "", PARITY, &files), "{root}");
        assert_eq!(found.len(), 1, "{root}: {found:?}");
        assert_eq!(found[0].1, "gate.exceptions.toml", "{root}");
        assert!(
            found[0].3.contains(&format!("which {read} does not list")),
            "{root}: {found:?}"
        );
    }
}

//! The generator `ledger/layout` seeds the manifest and the five shared files at the paths the
//! project configures, the files a tracked manifest lists, and nothing else; the seeded ledger
//! passes every rule that needs no option only the project can give.

use gate_sdk::GenerateOutput;

use crate::{Files, Seen, findings, input, keys, seen};

const MANIFEST: &str = "docs/ledger/manifest.json";

/// The seeded manifest at the plugin's defaults.
const DEFAULT_MANIFEST: &str = r#"{
  "canonicalRoot": "docs/ledger",
  "canonicalFiles": [
    "decision-index.md",
    "implementation-slices.md",
    "verification.md",
    "follow-ups.md",
    "production-blockers.md"
  ],
  "idFamilies": {},
  "forbiddenReferences": []
}
"#;

fn layout(options: &str, plugin_options: &str, files: &Files) -> GenerateOutput {
    gate_sdk::generate(
        &ledger::PLUGIN,
        files,
        "layout",
        "",
        options,
        plugin_options,
    )
    .expect("generates")
}

/// The written files as path, seed and text.
fn written(output: &GenerateOutput) -> Vec<(String, bool, String)> {
    output
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.seed,
                String::from_utf8(file.contents.clone()).expect("UTF-8"),
            )
        })
        .collect()
}

/// `files` with every file of `output` added, as `gate sync` writes the seeds into an empty tree.
fn seeded(files: Files, output: &GenerateOutput) -> Files {
    written(output)
        .into_iter()
        .fold(files, |files, (path, _, text)| files.with(&path, &text))
}

#[test]
fn an_empty_project_gets_the_manifest_and_the_five_shared_files_as_seeds() {
    let output = layout("{}", "{}", &Files::default());
    assert_eq!(
        written(&output),
        [
            (MANIFEST.to_owned(), true, DEFAULT_MANIFEST.to_owned()),
            (
                "docs/ledger/decision-index.md".to_owned(),
                true,
                "# Decision index\n\n| ID | Status | Title | Canonical file |\n| --- | --- | --- | --- |\n"
                    .to_owned()
            ),
            (
                "docs/ledger/implementation-slices.md".to_owned(),
                true,
                "# Implementation slices\n".to_owned()
            ),
            (
                "docs/ledger/verification.md".to_owned(),
                true,
                "# Verification\n".to_owned()
            ),
            (
                "docs/ledger/follow-ups.md".to_owned(),
                true,
                "# Follow-ups\n".to_owned()
            ),
            (
                "docs/ledger/production-blockers.md".to_owned(),
                true,
                "# Production blockers\n".to_owned()
            ),
        ]
    );
    assert_eq!(output.findings, []);
}

#[test]
fn the_seeds_follow_the_paths_the_project_configures() {
    let plugin_options = r#"{"decisions": "records/decisions.md", "implementation": "records/work/slices.md", "verification": "records/checks.md", "follow-ups": "records/checks.md", "blockers": "records/blockers.md"}"#;
    let output = layout(
        r#"{"manifest": "records/manifest.json"}"#,
        plugin_options,
        &Files::default(),
    );
    let paths: Vec<(String, bool)> = written(&output)
        .into_iter()
        .map(|(path, seed, _)| (path, seed))
        .collect();
    assert_eq!(
        paths,
        [
            ("records/manifest.json".to_owned(), true),
            ("records/decisions.md".to_owned(), true),
            ("records/work/slices.md".to_owned(), true),
            ("records/checks.md".to_owned(), true),
            ("records/blockers.md".to_owned(), true),
        ]
    );
    // The root is the deepest directory of every shared file, and a path two options name is
    // listed once.
    let manifest = &written(&output)[0].2;
    assert!(
        manifest.contains(
            "\"canonicalRoot\": \"records\",\n  \"canonicalFiles\": [\n    \"decisions.md\",\n    \"work/slices.md\",\n    \"checks.md\",\n    \"blockers.md\"\n  ],"
        ),
        "{manifest}"
    );

    let error = gate_sdk::generate(
        &ledger::PLUGIN,
        &Files::default(),
        "layout",
        "",
        r#"{"manifest": "./records/manifest.json"}"#,
        "{}",
    )
    .expect_err("an option not in normal form fails");
    assert!(
        error.message().contains(
            "[generate] ledger/layout manifest \"./records/manifest.json\" is not in normal form"
        ),
        "{error}"
    );
}

#[test]
fn a_tracked_manifest_gets_each_canonical_file_it_lists_seeded_with_a_title() {
    let manifest = DEFAULT_MANIFEST.replace(
        "\"production-blockers.md\"\n",
        "\"production-blockers.md\",\n    \"risk-register.md\",\n    \"../../../outside.md\"\n",
    );
    let output = layout("{}", "{}", &Files::new(&[(MANIFEST, manifest.as_str())]));
    let found: Vec<(String, bool, String)> = written(&output)
        .into_iter()
        .filter(|(path, _, _)| path == "docs/ledger/risk-register.md" || path.contains("outside"))
        .collect();
    assert_eq!(
        found,
        [(
            "docs/ledger/risk-register.md".to_owned(),
            true,
            "# Risk register\n".to_owned()
        )]
    );
    assert_eq!(written(&output).len(), 7);
    assert!(written(&output).iter().all(|(_, seed, _)| *seed));
}

/// Every rule that needs no option only the project can give, by rule, over `files`; `reuses` and
/// `closing-notes` with the option each requires. `record-status` needs `parity`, the project's
/// own plan, so it is not run.
fn every_rule(files: &Files) -> Vec<(&'static str, Vec<Seen>)> {
    [
        ("blockers", ""),
        ("closing-notes", r#"{"field":"`TEMPORARY_DELTA` records"}"#),
        ("decisions", ""),
        ("exception-decisions", ""),
        ("exception-owners", ""),
        ("manifest", ""),
        ("records", ""),
        ("reuses", r#"{"catalog":"docs/reference/mechanisms.md"}"#),
        ("rule-decisions", ""),
    ]
    .into_iter()
    .map(|(rule, options)| (rule, findings(rule, options, "{}", files)))
    .collect()
}

/// The seeded ledger is structurally complete: no rule reports a missing, misplaced or malformed
/// file. It is not content-complete: the files only the project can write are `#input`, and
/// `ledger/records` asks for the first implementation record and the first production blocker
/// record (`#none`), which no generic seed can hold.
#[test]
fn the_seeded_ledger_asks_only_for_what_the_project_writes() {
    let files = seeded(Files::default(), &layout("{}", "{}", &Files::default()));
    assert_eq!(
        every_rule(&files),
        [
            ("blockers", vec![]),
            ("closing-notes", vec![]),
            (
                "decisions",
                vec![input(
                    "decisions",
                    "docs/adr/decision-coverage.md",
                    "its option coverage"
                )]
            ),
            ("exception-decisions", vec![]),
            ("exception-owners", vec![]),
            ("manifest", vec![]),
            (
                "records",
                vec![
                    seen(
                        "record-shape",
                        "docs/ledger/implementation-slices.md",
                        None,
                        "docs/ledger/implementation-slices.md must contain at least one implementation record"
                    ),
                    seen(
                        "record-shape",
                        "docs/ledger/production-blockers.md",
                        None,
                        "docs/ledger/production-blockers.md must contain at least one production blocker record"
                    ),
                ]
            ),
            (
                "reuses",
                vec![input(
                    "reuses",
                    "docs/reference/mechanisms.md",
                    "its option catalog"
                )]
            ),
            ("rule-decisions", vec![]),
        ]
    );
}

/// The two record findings of the seeded ledger are the `#none` of each entry file, the key a
/// baseline or exception names them by.
#[test]
fn the_seeded_ledgers_record_findings_are_keyed_as_none() {
    let files = seeded(Files::default(), &layout("{}", "{}", &Files::default()));
    assert_eq!(
        keys("records", "", "{}", &files),
        [
            (
                "record-shape".to_owned(),
                "docs/ledger/implementation-slices.md".to_owned(),
                None,
                Some("implementation record#none".to_owned())
            ),
            (
                "record-shape".to_owned(),
                "docs/ledger/production-blockers.md".to_owned(),
                None,
                Some("production blocker record#none".to_owned())
            ),
        ]
    );
}

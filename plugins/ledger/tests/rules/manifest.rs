//! `ledger/manifest`.

use crate::{Files, findings, seen};

const MANIFEST: &str = "docs/ledger/manifest.json";
const INDEX: &str = "docs/ledger/decision-index.md";

const SHAPE: &str = r#"{
  "canonicalRoot": "docs/ledger",
  "canonicalFiles": ["decision-index.md"],
  "forbiddenReferences": [{ "label": "old ledger", "parts": ["docs", "old-ledger"] }],
  "idFamilies": {
    "D": { "pattern": "^\\| `D-[A-Z]+-\\d{3}`", "file": "decision-index.md", "unique": true },
    "R": { "pattern": "^- `R-\\d+`", "directory": "docs/ledger/records" }
  }
}"#;

fn tree() -> Files {
    Files::new(&[
        (MANIFEST, SHAPE),
        (INDEX, "| `D-ONE-001` | `accepted` | One | `one.md` |\n"),
        ("docs/ledger/records/a.md", "- `R-1`: first\n"),
        ("README.md", "# Readme\n"),
    ])
}

fn manifest(files: &Files) -> Vec<crate::Seen> {
    findings("manifest", "", "", files)
}

#[test]
fn a_well_formed_manifest_passes_and_an_untracked_one_turns_the_rule_off() {
    assert_eq!(manifest(&tree()), []);
    let files = tree()
        .without(MANIFEST)
        .with("README.md", "see docs/old-ledger\n");
    assert_eq!(manifest(&files), []);
}

#[test]
fn manifest_shape_invalid_json_missing_fields_and_bad_families() {
    let found = manifest(&tree().with(MANIFEST, "{"));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "manifest-shape");
    assert!(
        found[0]
            .3
            .starts_with("docs/ledger/manifest.json is not valid JSON: EOF while parsing"),
        "{found:#?}"
    );

    assert_eq!(
        manifest(&tree().with(MANIFEST, r#"{"canonicalRoot":"docs/ledger"}"#)),
        [seen(
            "manifest-shape",
            MANIFEST,
            None,
            "docs/ledger/manifest.json contains malformed required ledger manifest fields"
        )]
    );

    let files = tree().edit(MANIFEST, r#""file": "decision-index.md","#, "");
    assert_eq!(
        manifest(&files),
        [seen(
            "manifest-shape",
            MANIFEST,
            None,
            "docs/ledger/manifest.json contains malformed D-family definition"
        )]
    );

    let files = tree().edit(MANIFEST, r"^- `R-\\d+`", r"^- `R-(?<=x)\\d+`");
    let found = manifest(&files);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0]
            .3
            .starts_with("docs/ledger/manifest.json contains invalid R-family pattern: "),
        "{found:#?}"
    );

    let files = tree().edit(
        MANIFEST,
        r#"["decision-index.md"]"#,
        r#"["decision-index.md", "decision-index.md"]"#,
    );
    assert_eq!(
        manifest(&files),
        [seen(
            "manifest-shape",
            MANIFEST,
            None,
            "docs/ledger/manifest.json contains duplicate canonicalFiles entries"
        )]
    );
}

#[test]
fn manifest_file_a_required_file_and_a_family_directory_are_missing() {
    let files = tree()
        .edit(
            MANIFEST,
            r#"["decision-index.md"]"#,
            r#"["decision-index.md", "../../../gone.md", "plan.md"]"#,
        )
        .without("docs/ledger/records/a.md");
    assert_eq!(
        manifest(&files),
        [
            seen(
                "manifest-file",
                MANIFEST,
                None,
                "docs/ledger/../../../gone.md is required by the ledger manifest"
            ),
            seen(
                "manifest-file",
                "docs/ledger/plan.md",
                None,
                "docs/ledger/plan.md is required by the ledger manifest"
            ),
            seen(
                "manifest-file",
                "docs/ledger/records",
                None,
                "docs/ledger/records is required by the R-family definition"
            ),
        ]
    );
}

#[test]
fn id_family_and_record_unique() {
    let files = tree()
        .with(
            INDEX,
            "| `D-ONE-001` | `accepted` | One | `one.md` |\n| `D-ONE-001` | `accepted` | One | `one.md` |\n",
        )
        .with("docs/ledger/records/a.md", "```\n- `R-1`: fenced\n```\n");
    assert_eq!(
        manifest(&files),
        [
            seen(
                "record-unique",
                INDEX,
                None,
                "docs/ledger/decision-index.md contains duplicate D records"
            ),
            seen(
                "id-family",
                "docs/ledger/records",
                None,
                "docs/ledger/records has no R-family ledger IDs"
            ),
        ]
    );
}

#[test]
fn forbidden_reference_in_text_files_only() {
    let files = tree()
        .with("README.md", "# Readme\n\nsee docs/old-ledger/a.md\n")
        .with("deploy/Dockerfile.dev", "COPY docs/old-ledger /x\n")
        .with("LICENSE", "docs/old-ledger\n")
        .with("node_modules/pkg/a.md", "docs/old-ledger\n")
        .with("assets/logo.png", "docs/old-ledger\n");
    assert_eq!(
        manifest(&files),
        [
            seen(
                "forbidden-reference",
                "LICENSE",
                Some(1),
                "LICENSE references forbidden ledger path: old ledger"
            ),
            seen(
                "forbidden-reference",
                "README.md",
                Some(3),
                "README.md references forbidden ledger path: old ledger"
            ),
            seen(
                "forbidden-reference",
                "deploy/Dockerfile.dev",
                Some(1),
                "deploy/Dockerfile.dev references forbidden ledger path: old ledger"
            ),
        ]
    );
    let options =
        r#"{"skip-folders":["deploy"],"text-extensions":[".png"],"text-file-prefixes":[]}"#;
    let found: Vec<String> = findings("manifest", options, "", &files)
        .into_iter()
        .map(|finding| finding.1)
        .collect();
    assert_eq!(found, ["LICENSE", "assets/logo.png"]);
}

#[test]
fn a_forbidden_reference_needs_parts_and_a_string_label() {
    for (from, to) in [
        (r#""parts": ["docs", "old-ledger"]"#, r#""parts": []"#),
        (r#""label": "old ledger""#, r#""label": 7"#),
        (r#""label": "old ledger", "#, ""),
    ] {
        assert_eq!(
            manifest(&tree().edit(MANIFEST, from, to)),
            [seen(
                "manifest-shape",
                MANIFEST,
                None,
                "docs/ledger/manifest.json contains malformed required ledger manifest fields"
            )],
            "{to}"
        );
    }
}

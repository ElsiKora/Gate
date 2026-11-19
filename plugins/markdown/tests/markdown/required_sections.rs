use crate::fixture::{Fixture, lines};

const OPTIONS: &str = r#"{"sections": [
    {"path": "docs/areas/README\\.md", "sections": ["Map", "Owners"]},
    {"path": "docs/areas/[a-z]+\\.md", "sections": ["Purpose", "Sources", "Links"]},
    {"path": "docs/areas/.+\\.md", "sections": ["Unreachable"]}
]}"#;

#[test]
fn a_page_with_every_section_passes_and_each_missing_one_is_reported_in_order() {
    let fixture = Fixture::new(&[
        (
            "docs/areas/billing.md",
            "## Purpose\n## Sources ##\n   ##\tLinks  \n",
        ),
        (
            "docs/areas/search.md",
            "## Purpose\n```md\n## Sources\n```\n<!--\n## Links\n-->\n### Links\n",
        ),
        ("docs/areas/README.md", "## Map\n"),
        ("docs/areas/other-name.md", "# none\n"),
        ("docs/overview.md", "# none\n"),
    ]);
    let findings = fixture.check("required-sections", OPTIONS).expect("runs");
    assert_eq!(
        lines(&findings),
        [
            "docs/areas/README.md docs/areas/README.md is missing module documentation section: Owners",
            "docs/areas/other-name.md docs/areas/other-name.md is missing module documentation section: Unreachable",
            "docs/areas/search.md docs/areas/search.md is missing module documentation section: Sources",
            "docs/areas/search.md docs/areas/search.md is missing module documentation section: Links",
        ]
    );
    assert_eq!(
        findings[0].target.as_deref(),
        Some("docs/areas/README.md:Owners")
    );
}

#[test]
fn a_pattern_matches_the_whole_path_only() {
    let fixture = Fixture::new(&[
        ("docs/areas/billing.md.orig", ""),
        ("docs/x/docs/areas/billing.md", ""),
    ]);
    let findings = fixture
        .check(
            "required-sections",
            r#"{"roots": [""], "extra-files": ["docs/areas/billing.md.orig"],
                "sections": [{"path": "docs/areas/[a-z]+\\.md", "sections": ["Purpose"]}]}"#,
        )
        .expect("runs");
    assert_eq!(lines(&findings), Vec::<String>::new());
}

#[test]
fn without_sections_the_rule_is_silent() {
    let fixture = Fixture::new(&[("docs/a.md", "# A\n")]);
    assert_eq!(fixture.check("required-sections", "{}").expect("runs"), []);
    let empty = r#"{"sections": [{"path": "docs/a\\.md", "sections": []}]}"#;
    assert_eq!(fixture.check("required-sections", empty).expect("runs"), []);
}

#[test]
fn a_pattern_that_is_invalid_alone_is_a_configuration_error() {
    for pattern in [r"docs/(", r"a)|(b"] {
        let options = format!(
            r#"{{"sections": [{{"path": {}, "sections": ["A"]}}]}}"#,
            serde_json::to_string(pattern).expect("a string")
        );
        let error = Fixture::new(&[])
            .check("required-sections", &options)
            .expect_err("invalid");
        assert!(
            error
                .message()
                .contains(&format!("sections has an invalid path pattern {pattern:?}")),
            "{error}"
        );
    }
}

#[test]
fn an_entry_needs_both_keys() {
    let error = Fixture::new(&[])
        .check("required-sections", r#"{"sections": [{"path": "a"}]}"#)
        .expect_err("missing sections");
    assert!(
        error.message().contains("missing field `sections`"),
        "{error}"
    );
}

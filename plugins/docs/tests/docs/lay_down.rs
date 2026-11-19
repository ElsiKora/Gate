//! The generator `docs/layout` seeds exactly the required indexes of the kind; the core writes a
//! seed only where nothing exists, so the generator writes the same set on any tree.

use crate::fixture::{Fixture, kind};

const MARKER: &str = "<!-- gate:seed: replace this page's text, then delete this line -->";

const NINE: [&str; 9] = [
    "docs/README.md",
    "docs/start-here.md",
    "docs/architecture/README.md",
    "docs/modules/README.md",
    "docs/adr/README.md",
    "docs/reference/README.md",
    "docs/guide/README.md",
    "docs/testing/README.md",
    "docs/operations/README.md",
];

#[test]
fn an_empty_project_gets_the_required_indexes_of_its_kind_as_seeds() {
    for (name, count) in [
        ("service", 9),
        ("frontend", 9),
        ("infrastructure", 9),
        ("library", 9),
        ("monorepo-root", 8),
    ] {
        let output = Fixture::new(&[]).generate(&kind(name)).expect("generates");
        let paths: Vec<&str> = output.files.iter().map(|file| file.path.as_str()).collect();
        let expected: Vec<&str> = NINE
            .iter()
            .copied()
            .filter(|path| name != "monorepo-root" || *path != "docs/modules/README.md")
            .collect();
        assert_eq!(paths, expected, "{name}");
        assert_eq!(paths.len(), count, "{name}");
        for file in &output.files {
            assert!(file.seed, "{name}: {} is a seed", file.path);
            let text = String::from_utf8(file.contents.clone()).expect("UTF-8");
            assert_eq!(text.lines().nth(2), Some(MARKER), "{name}: {}", file.path);
        }
        assert_eq!(output.findings, Vec::new(), "{name}");
    }
}

#[test]
fn each_seed_carries_its_kinds_sections_in_order_and_the_index_its_entry_table() {
    let output = Fixture::new(&[])
        .generate(&kind("frontend"))
        .expect("generates");
    let text = |path: &str| {
        let file = output
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("seeded");
        String::from_utf8(file.contents.clone()).expect("UTF-8")
    };
    let headings = |path: &str| -> Vec<String> {
        text(path)
            .lines()
            .filter(|line| line.starts_with("## "))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(
        headings("docs/architecture/README.md"),
        [
            "## System shape",
            "## Routing",
            "## State",
            "## Rendering",
            "## API access"
        ]
    );
    let index = text("docs/README.md");
    let rows: Vec<&str> = index
        .lines()
        .filter(|line| line.starts_with("| ["))
        .map(|line| line.split(']').next().unwrap_or(line))
        .collect();
    assert_eq!(
        rows,
        [
            "| [start-here.md",
            "| [architecture/",
            "| [modules/",
            "| [adr/",
            "| [reference/",
            "| [guide/",
            "| [testing/",
            "| [operations/"
        ]
    );
    assert!(text("docs/adr/README.md").contains("```markdown\n# NNNN: "));

    let root = Fixture::new(&[])
        .generate(&kind("monorepo-root"))
        .expect("generates");
    let root_index = root
        .files
        .iter()
        .find(|file| file.path == "docs/README.md")
        .expect("seeded");
    let root_index = String::from_utf8(root_index.contents.clone()).expect("UTF-8");
    assert!(root_index.contains("\n## Members\n"), "{root_index}");
    assert!(!root_index.contains("[modules/]"), "{root_index}");
}

#[test]
fn the_seeds_of_a_kind_pass_the_rule_once_filled() {
    for name in [
        "service",
        "frontend",
        "infrastructure",
        "library",
        "monorepo-root",
    ] {
        let options = kind(name);
        let seeded =
            Fixture::from_output(&Fixture::new(&[]).generate(&options).expect("generates"));
        let findings = seeded.check(&options).expect("checks");
        let aspects: Vec<&str> = findings
            .iter()
            .map(|finding| finding.aspect.as_str())
            .collect();
        let expected = if name == "monorepo-root" { 8 } else { 9 };
        assert_eq!(aspects, vec!["unfilled"; expected], "{name}");
        let filled = seeded.replaced(&format!("{MARKER}\n"), "");
        assert_eq!(
            filled.check(&options).expect("checks"),
            Vec::new(),
            "{name}"
        );
    }
}

#[test]
fn a_kind_that_is_missing_or_unknown_is_a_configuration_error_listing_the_five() {
    for options in ["{}", r#"{"kind": "backend"}"#] {
        let error = Fixture::new(&[])
            .generate(options)
            .expect_err("no kind, no standard");
        assert!(error.is_config(), "{options}: {error}");
        assert!(
            error
                .to_string()
                .contains("one of service, frontend, infrastructure, library, monorepo-root"),
            "{options}: {error}"
        );
    }
}

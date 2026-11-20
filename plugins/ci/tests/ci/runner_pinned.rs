use crate::fixture::{Fixture, aspects, lines};

const WORKFLOW: &str = "\
jobs:
  linux:
    runs-on: ubuntu-24.04
  matrix:
    runs-on: ${{ matrix.runner }}
  hosted:
    runs-on: [self-hosted, linux]
  latest:
    runs-on: ubuntu-latest
  list:
    runs-on:
      - macos-latest
  group:
    runs-on:
      group: large
      labels: [windows-latest]
";

#[test]
fn moving_labels_are_reported_at_their_line_and_versions_and_expressions_pass() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", WORKFLOW)]);
    let findings = fixture.check("runner-pinned", "{}").expect("runs");
    assert_eq!(
        aspects(&findings),
        [
            "- 9 latest:ubuntu-latest",
            "- 12 list:macos-latest",
            "- 16 group:windows-latest",
        ]
    );
    assert_eq!(
        lines(&findings)[0],
        ".github/workflows/ci.yml:9 job `latest` runs on `ubuntu-latest`, a label that moves to new images"
    );
    assert_eq!(
        findings[0].target.as_deref(),
        Some(".github/workflows/ci.yml:latest")
    );
}

#[test]
fn the_moving_option_names_the_labels() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", WORKFLOW)]);
    let findings = fixture
        .check("runner-pinned", r#"{"moving": ["self-hosted"]}"#)
        .expect("runs");
    assert_eq!(aspects(&findings), ["- 7 hosted:self-hosted"]);

    let error = fixture
        .check("runner-pinned", r#"{"moving": ["a["]}"#)
        .expect_err("not a glob");
    assert!(error.message().contains("is not a valid glob"), "{error}");
}

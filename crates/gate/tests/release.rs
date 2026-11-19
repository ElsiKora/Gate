//! `scripts/changelog-section.sh`, which gives a release its notes: it finds the section of the
//! tag's version, `#` or `##`, linked or plain, keeps its `###` parts and its code blocks (whose
//! lines are never headings), and fails for a version the changelog has no section for, or an
//! empty one, so that no release goes out without its notes.

use std::path::Path;
use std::process::{Command, Output};

const CHANGELOG: &str = "<!--
A comment.
-->

# [1.2.0](https://example.invalid/compare/v1.1.1...v1.2.0) (2026-02-14)

### Features

- **scope:** one

## [1.1.1](https://example.invalid/compare/v1.1.0...v1.1.1) (2026-02-01)

### Bug Fixes

- **scope:** two

## 1.1.0

- three, with an example:

  ```markdown
  ## Not a release
  ```

  ~~~
  # Nor this
  ~~~

## 1.0.0

# 0.9.0
";

fn section(version: &str) -> Output {
    let directory = tempfile::tempdir().expect("directory");
    let changelog = directory.path().join("CHANGELOG.md");
    std::fs::write(&changelog, CHANGELOG).expect("changelog");
    Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/changelog-section.sh"))
        .arg(&changelog)
        .arg(version)
        .output()
        .expect("sh")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn the_section_of_a_version_ends_at_the_next_release_heading() {
    for (version, expected) in [
        ("1.2.0", "### Features\n\n- **scope:** one\n"),
        ("1.1.1", "### Bug Fixes\n\n- **scope:** two\n"),
        (
            "1.1.0",
            "- three, with an example:\n\n  ```markdown\n  ## Not a release\n  ```\n\n  ~~~\n  # Nor this\n  ~~~\n",
        ),
    ] {
        let output = section(version);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), expected, "{version}");
    }
}

#[test]
fn a_missing_or_empty_section_fails() {
    for (version, message) in [
        ("1.0.0", "the section for 1.0.0"),
        ("1.3.0", "has no section for 1.3.0"),
        ("1.2", "has no section for 1.2"),
    ] {
        let output = section(version);
        assert_eq!(output.status.code(), Some(1), "{version}");
        assert!(text(&output.stdout).is_empty(), "{version}");
        assert!(
            text(&output.stderr).contains(message),
            "{version}: {}",
            text(&output.stderr)
        );
    }
}

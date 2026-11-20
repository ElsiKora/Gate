use crate::fixture::{Fixture, aspects, lines};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn one_setup_from_a_version_file_with_a_cache_passes() {
    let workflow = format!(
        "\
jobs:
  build:
    steps:
      - uses: pnpm/action-setup@{SHA} # v4.1.0
      - uses: actions/setup-node@{SHA} # v4.4.0
        with:
          node-version-file: .nvmrc
          cache: pnpm
  rust:
    steps:
      - run: cargo build
"
    );
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", &workflow)]);
    assert_eq!(
        lines(&fixture.check("node-setup", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

#[test]
fn a_literal_version_a_missing_cache_and_a_second_setup_are_reported() {
    let workflow = format!(
        "\
jobs:
  build:
    steps:
      - uses: actions/setup-node@{SHA} # v4.4.0
        with:
          node-version: 24
          cache: npm
      - uses: actions/setup-node@{SHA} # v4.4.0
        with:
          node-version-file: .nvmrc
"
    );
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", &workflow)]);
    let findings = fixture.check("node-setup", "{}").expect("runs");
    assert_eq!(
        aspects(&findings),
        [
            "version-file 4 build:1",
            "once 8 build:2",
            "cache 8 build:2"
        ]
    );
    assert_eq!(
        lines(&findings),
        [
            ".github/workflows/ci.yml:4 job `build` sets up Node without node-version-file",
            ".github/workflows/ci.yml:8 job `build` sets up Node again",
            ".github/workflows/ci.yml:8 job `build` sets up Node without a cache",
        ]
    );
}

#[test]
fn a_version_from_the_matrix_passes() {
    let workflow = format!(
        "\
jobs:
  test:
    strategy:
      matrix:
        node: [22, 24]
    steps:
      - uses: actions/setup-node@{SHA} # v4.4.0
        with:
          node-version: ${{{{ matrix.node }}}}
          cache: npm
"
    );
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", &workflow)]);
    assert_eq!(
        lines(&fixture.check("node-setup", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

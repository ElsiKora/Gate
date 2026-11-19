use crate::fixture::{Fixture, lines};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn commits_digests_and_local_actions_pass() {
    let workflow = format!(
        "\
name: CI
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@{SHA} # v4
      - uses: ./.github/actions/setup
      - uses: docker://alpine@sha256:{DIGEST}
      - run: make
  reuse:
    uses: example/workflows/.github/workflows/build.yml@{SHA}
"
    );
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", &workflow)]);
    assert_eq!(
        lines(&fixture.check("actions-pinned", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

#[test]
fn tags_branches_short_shas_and_bare_images_are_reported_at_their_line() {
    let workflow = format!(
        "\
on: push
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - uses: actions/cache@{}
      - uses: actions/setup-node@{}
      - uses: docker://alpine:3
      - uses:
  reuse:
    uses: example/workflows/.github/workflows/build.yml@main
",
        &SHA[..7],
        SHA.to_uppercase()
    );
    let fixture = Fixture::new(&[
        (".github/workflows/build.yaml", &workflow),
        (
            ".github/workflows/nested/skip.yml",
            "jobs: {a: {uses: x@v1}}\n",
        ),
        (".github/workflows/notes.txt", "jobs: {a: {uses: x@v1}}\n"),
    ]);
    let findings = fixture.check("actions-pinned", "{}").expect("runs");
    let path = ".github/workflows/build.yaml";
    assert_eq!(
        lines(&findings),
        [
            format!("{path}:5 `actions/checkout@v4` is not pinned by a full commit SHA"),
            format!("{path}:6 `actions/cache@0123456` is not pinned by a full commit SHA"),
            format!(
                "{path}:7 `actions/setup-node@{}` is not pinned by a full commit SHA",
                SHA.to_uppercase()
            ),
            format!("{path}:8 `docker://alpine:3` is not pinned by a full commit SHA"),
            // An empty value starts where the parser meets the next token.
            format!("{path}:10 `` is not pinned by a full commit SHA"),
            format!(
                "{path}:11 `example/workflows/.github/workflows/build.yml@main` is not pinned by a full commit SHA"
            ),
        ]
    );
    assert_eq!(
        findings[0].target.as_deref(),
        Some(".github/workflows/build.yaml:actions/checkout@v4")
    );
}

#[test]
fn the_workflows_glob_chooses_the_files() {
    let fixture = Fixture::new(&[
        (".github/workflows/ci.yml", "jobs: {a: {uses: x@v1}}\n"),
        ("ci/pipelines/deep/release.yml", "jobs: {a: {uses: y@v1}}\n"),
    ]);
    let findings = fixture
        .check("actions-pinned", r#"{"workflows": "ci/**/*.yml"}"#)
        .expect("runs");
    assert_eq!(
        lines(&findings),
        ["ci/pipelines/deep/release.yml:1 `y@v1` is not pinned by a full commit SHA"]
    );
}

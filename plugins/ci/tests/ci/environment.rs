use crate::fixture::{Fixture, aspects, lines};

const WORKFLOW: &str = "\
jobs:
  release:
    environment: release
    permissions:
      contents: write
  comment:
    permissions:
      pull-requests: write
      contents: read
  publish:
    permissions:
      contents: read
      packages: write
      id-token: write
  everything:
    permissions: write-all
  reuse:
    permissions:
      contents: write
    uses: ./.github/workflows/release.yml
";

#[test]
fn a_job_that_writes_outside_the_checks_without_an_environment_is_reported() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", WORKFLOW)]);
    let findings = fixture.check("environment", "{}").expect("runs");
    assert_eq!(aspects(&findings), ["- 12 publish", "- 16 everything"]);
    assert_eq!(
        lines(&findings),
        [
            ".github/workflows/ci.yml:12 job `publish` writes `packages`, `id-token` and runs in no environment",
            ".github/workflows/ci.yml:16 job `everything` writes `contents`, `deployments`, `id-token`, `packages` and runs in no environment",
        ]
    );
}

#[test]
fn the_scopes_option_names_the_scopes_that_need_an_environment() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", WORKFLOW)]);
    let findings = fixture
        .check("environment", r#"{"scopes": ["pull-requests"]}"#)
        .expect("runs");
    assert_eq!(aspects(&findings), ["- 8 comment", "- 16 everything"]);
}

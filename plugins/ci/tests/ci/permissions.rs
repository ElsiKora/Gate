use crate::fixture::{Fixture, aspects, lines};

#[test]
fn read_only_workflow_permissions_and_a_job_s_own_pass() {
    let workflow = "\
permissions: {}
jobs:
  build:
    permissions:
      contents: read
    steps:
      - run: make
  publish:
    permissions:
      packages: write
    steps:
      - run: make publish
  reuse:
    permissions: {}
    uses: ./.github/workflows/build.yml
";
    let other = "permissions:\n  contents: read\njobs:\n  a:\n    permissions: {}\n";
    let fixture = Fixture::new(&[
        (".github/workflows/ci.yml", workflow),
        (".github/workflows/other.yml", other),
    ]);
    assert_eq!(
        lines(&fixture.check("permissions", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

#[test]
fn a_workflow_write_a_job_without_permissions_and_a_shorthand_are_reported() {
    let workflow = "\
permissions:
  contents: write
  issues: read
jobs:
  build:
    steps:
      - run: make
  release:
    permissions: write-all
    steps:
      - run: make release
";
    let fixture = Fixture::new(&[
        (".github/workflows/ci.yml", workflow),
        (
            ".github/workflows/all.yml",
            "permissions: read-all\njobs: {}\n",
        ),
    ]);
    let findings = fixture.check("permissions", "{}").expect("runs");
    assert_eq!(
        aspects(&findings),
        [
            "shorthand 1 workflow",
            "workflow-write 2 contents",
            "job-missing 6 build",
            "shorthand 9 release",
        ]
    );
    assert_eq!(
        lines(&findings)[1],
        ".github/workflows/ci.yml:2 the workflow's own permissions grant `contents: write` to every job"
    );
    assert_eq!(
        findings[2].target.as_deref(),
        Some(".github/workflows/ci.yml:build")
    );
    assert_eq!(
        lines(&findings)[3],
        ".github/workflows/ci.yml:9 job `release` grants `write-all`, every scope at once"
    );
}

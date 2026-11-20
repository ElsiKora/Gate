use crate::fixture::{Fixture, aspects, lines};

#[test]
fn a_group_that_cancels_only_pull_requests_passes() {
    let generated = "\
on: [push, pull_request]
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
jobs: {}
";
    let scalar = "on: push\nconcurrency: deploy-${{ github.ref }}\njobs: {}\n";
    let reviews = "\
on:
  pull_request:
  merge_group:
concurrency:
  group: review-${{ github.ref }}
  cancel-in-progress: true
jobs: {}
";
    let fixture = Fixture::new(&[
        (".github/workflows/ci.yml", generated),
        (".github/workflows/deploy.yml", scalar),
        (".github/workflows/review.yml", reviews),
    ]);
    assert_eq!(
        lines(&fixture.check("concurrency", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

#[test]
fn a_missing_group_and_an_unconditional_cancel_of_a_push_are_reported() {
    let cancels = "\
on: [push, pull_request]
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs: {}
";
    let fixture = Fixture::new(&[
        (".github/workflows/ci.yml", cancels),
        (".github/workflows/none.yml", "on: push\njobs: {}\n"),
        (
            ".github/workflows/empty.yml",
            "on: push\nconcurrency:\n  cancel-in-progress: false\njobs: {}\n",
        ),
    ]);
    let findings = fixture.check("concurrency", "{}").expect("runs");
    assert_eq!(
        aspects(&findings),
        ["cancel 4 -", "missing 3 -", "missing 1 -"]
    );
    assert_eq!(
        lines(&findings),
        [
            ".github/workflows/ci.yml:4 cancel-in-progress is true, so a newer run also cancels a running `push` run",
            ".github/workflows/empty.yml:3 the workflow has no concurrency group",
            ".github/workflows/none.yml:1 the workflow has no concurrency group",
        ]
    );
}

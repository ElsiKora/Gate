use crate::fixture::{Fixture, aspects, lines};

const WORKFLOW: &str = "\
jobs:
  build:
    timeout-minutes: 45
    steps:
      - run: make
  reuse:
    uses: ./.github/workflows/build.yml
  matrix:
    timeout-minutes: ${{ matrix.minutes }}
    steps:
      - run: make
  test:
    steps:
      - run: make test
";

#[test]
fn a_job_with_steps_and_no_timeout_is_reported_and_a_reusable_call_is_not() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", WORKFLOW)]);
    let findings = fixture.check("timeout", "{}").expect("runs");
    assert_eq!(aspects(&findings), ["missing 13 test"]);
    assert_eq!(
        lines(&findings),
        [".github/workflows/ci.yml:13 job `test` sets no timeout-minutes"]
    );
    assert_eq!(
        findings[0].target.as_deref(),
        Some(".github/workflows/ci.yml:test")
    );
}

#[test]
fn max_minutes_bounds_a_written_timeout_and_leaves_an_expression() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", WORKFLOW)]);
    let findings = fixture
        .check("timeout", r#"{"max-minutes": 30}"#)
        .expect("runs");
    assert_eq!(aspects(&findings), ["over-max 3 build", "missing 13 test"]);
    assert_eq!(
        lines(&findings)[0],
        ".github/workflows/ci.yml:3 job `build` sets timeout-minutes 45, above 30"
    );
    let findings = fixture
        .check("timeout", r#"{"max-minutes": 45}"#)
        .expect("runs");
    assert_eq!(aspects(&findings), ["missing 13 test"]);
}

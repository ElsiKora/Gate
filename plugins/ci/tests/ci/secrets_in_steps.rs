use crate::fixture::{Fixture, aspects, lines};

#[test]
fn a_secret_in_a_step_passes() {
    let workflow = "\
env:
  MODE: release
jobs:
  deploy:
    env:
      REGION: eu
    steps:
      - run: ./deploy
        env:
          TOKEN: ${{ secrets.DEPLOY_TOKEN }}
      - uses: ./.github/actions/notify
        with:
          token: ${{ secrets.NOTIFY_TOKEN }}
  reuse:
    uses: ./.github/workflows/build.yml
    secrets:
      token: ${{ secrets.BUILD_TOKEN }}
";
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", workflow)]);
    assert_eq!(
        lines(&fixture.check("secrets-in-steps", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

#[test]
fn a_secret_in_a_workflow_or_job_env_and_inherited_secrets_are_reported() {
    let workflow = "\
env:
  TOKEN: ${{ secrets.TOKEN }}
jobs:
  scan:
    env:
      SCAN_TOKEN: ${{ secrets.SCAN_TOKEN }}
      LEVEL: high
    steps:
      - run: ./scan
  reuse:
    uses: ./.github/workflows/build.yml
    secrets: inherit
";
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", workflow)]);
    let findings = fixture.check("secrets-in-steps", "{}").expect("runs");
    assert_eq!(
        aspects(&findings),
        [
            "workflow-env 2 TOKEN",
            "job-env 6 scan:SCAN_TOKEN",
            "inherit 12 reuse"
        ]
    );
    assert_eq!(
        lines(&findings),
        [
            ".github/workflows/ci.yml:2 the workflow's env `TOKEN` holds a secret every job sees",
            ".github/workflows/ci.yml:6 job `scan`'s env `SCAN_TOKEN` holds a secret every step sees",
            ".github/workflows/ci.yml:12 job `reuse` passes every secret of the repository to the workflow it calls",
        ]
    );
}

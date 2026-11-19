use crate::fixture::{Fixture, lines};

const BUILD: &str = "name: Build\non: push\njobs: {}\n";

#[test]
fn a_trigger_naming_existing_workflows_by_name_or_path_passes() {
    let fixture = Fixture::new(&[
        (".github/workflows/build.yml", BUILD),
        (".github/workflows/lint.yml", "on: push\njobs: {}\n"),
        (
            ".github/workflows/deploy.yml",
            "on:\n  workflow_run:\n    workflows: [Build, .github/workflows/lint.yml]\n    types: [completed]\n",
        ),
        (
            ".github/workflows/notify.yml",
            "on:\n  workflow_run:\n    workflows: Build\n",
        ),
    ]);
    let findings = fixture.check("workflow-run-target", "{}").expect("runs");
    assert_eq!(lines(&findings), Vec::<String>::new());
}

#[test]
fn an_unknown_name_and_a_trigger_without_workflows_are_reported() {
    let fixture = Fixture::new(&[
        (".github/workflows/build.yml", BUILD),
        (
            ".github/workflows/deploy.yml",
            "on:\n  workflow_run:\n    workflows:\n      - Build\n      - build\n      - Tests\n",
        ),
        (
            ".github/workflows/notify.yml",
            "on:\n  workflow_run:\n    types: [completed]\n",
        ),
    ]);
    let findings = fixture.check("workflow-run-target", "{}").expect("runs");
    assert_eq!(
        lines(&findings),
        [
            ".github/workflows/deploy.yml:5 workflow_run names `build`, which no workflow defines",
            ".github/workflows/deploy.yml:6 workflow_run names `Tests`, which no workflow defines",
            ".github/workflows/notify.yml:3 workflow_run names no workflows",
        ]
    );
    assert_eq!(
        findings[0].target.as_deref(),
        Some(".github/workflows/deploy.yml:build")
    );
    assert_eq!(
        findings[2].target.as_deref(),
        Some(".github/workflows/notify.yml")
    );
}

#[test]
fn a_workflow_outside_the_glob_is_not_a_name() {
    let fixture = Fixture::new(&[
        ("ci/build.yml", BUILD),
        (
            ".github/workflows/deploy.yml",
            "on:\n  workflow_run:\n    workflows: [Build]\n",
        ),
    ]);
    let findings = fixture.check("workflow-run-target", "{}").expect("runs");
    assert_eq!(
        lines(&findings),
        [".github/workflows/deploy.yml:3 workflow_run names `Build`, which no workflow defines"]
    );
}

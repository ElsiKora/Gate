use crate::fixture::{Fixture, aspects, lines};

#[test]
fn named_workflows_jobs_and_steps_in_their_files_pass() {
    let workflow = "\
name: Release And Publish
jobs:
  build:
    name: Build
    steps:
      - name: Compile
        run: make
  test:
    name: Test ${{ matrix.os }}
    steps:
      - name: Test
        run: make test
  test-again:
    name: Test ${{ matrix.os }}
    uses: ./.github/workflows/test.yml
";
    let fixture = Fixture::new(&[
        (".github/workflows/release-and-publish.yml", workflow),
        (".github/workflows/test.yaml", "name: Test\njobs: {}\n"),
    ]);
    assert_eq!(
        lines(&fixture.check("names", "{}").expect("runs")),
        Vec::<String>::new()
    );
}

#[test]
fn each_missing_name_a_file_off_its_name_a_duplicate_and_a_snake_id_are_reported() {
    let workflow = "\
name: Release And Publish
jobs:
  build_image:
    name: Build
    steps:
      - run: make
  build:
    name: Build
    steps:
      - name: Compile
        run: make
  test:
    uses: ./.github/workflows/test.yml
";
    let fixture = Fixture::new(&[
        (".github/workflows/release.yml", workflow),
        (".github/workflows/unnamed.yml", "jobs: {}\n"),
        (
            ".github/workflows/zz-release.yml",
            "name: Release And Publish\njobs: {}\n",
        ),
    ]);
    let findings = fixture.check("names", "{}").expect("runs");
    assert_eq!(
        aspects(&findings),
        [
            "file 1 -",
            "job-id 3 build_image",
            "step 6 build_image:1",
            "duplicate 8 build",
            "job 13 test",
            "workflow 1 -",
            "file 1 -",
            "duplicate 1 Release And Publish",
        ]
    );
    assert_eq!(
        lines(&findings),
        [
            ".github/workflows/release.yml:1 the workflow `Release And Publish` is in `release.yml`; its name in kebab case is `release-and-publish`",
            ".github/workflows/release.yml:3 job id `build_image` is not kebab case",
            ".github/workflows/release.yml:6 step 1 of job `build_image` has no name",
            ".github/workflows/release.yml:8 job `build` is named `Build`, as job `build_image` is",
            ".github/workflows/release.yml:13 job `test` has no name",
            ".github/workflows/unnamed.yml:1 the workflow has no name",
            ".github/workflows/zz-release.yml:1 the workflow `Release And Publish` is in `zz-release.yml`; its name in kebab case is `release-and-publish`",
            ".github/workflows/zz-release.yml:1 the workflow is named `Release And Publish`, as .github/workflows/release.yml is",
        ]
    );
}

use crate::repo::{Repo, lines, merged, settings, short};

const RELEASE: &str = "chore(release): 1.2.0 [skip ci]";

#[test]
fn directives_and_trailers_are_reported_on_every_commit_but_the_release() {
    let mut repo = Repo::default();
    let old = repo.commit(&[], "start [skip ci]", &[("a", Some("1"))]);
    let activation = repo.commit(&[&old], "activate", &[("a", Some("2"))]);
    let branch = repo.commit(&[&activation], "feat: work [CI SKIP]", &[("b", Some("1"))]);
    let merge = repo.commit(
        &[&activation, &branch],
        "Merge branch 'feat/work'",
        &[("b", Some("1"))],
    );
    let release = repo.commit(&[&merge], RELEASE, &[("CHANGELOG.md", Some("1.2.0"))]);
    let trailer = repo.commit(
        &[&release],
        "docs: note\n\nSkip-Checks: true",
        &[("docs/a.md", Some("x"))],
    );
    repo.set_ref("refs/remotes/origin/main", &trailer);
    let side_release = repo.commit(&[&trailer], RELEASE, &[("c", Some("1"))]);
    let local = repo.commit(&[&side_release], "wip [no ci]", &[("d", Some("1"))]);
    repo.set_head(&local);

    let findings = repo
        .check("no-ci-skip", &settings(&activation), "{}")
        .expect("runs");
    let mut found = lines(&findings);
    found.sort();
    let mut expected = vec![
        format!(
            " commit:{branch} {} carries the forbidden CI-skip directive [CI SKIP]",
            short(&branch)
        ),
        format!(
            " commit:{trailer} {} carries the forbidden CI-skip directive Skip-Checks: true",
            short(&trailer)
        ),
        format!(
            " commit:{side_release} {} carries the forbidden CI-skip directive [skip ci]",
            short(&side_release)
        ),
        format!(
            " commit:{local} {} carries the forbidden CI-skip directive [no ci]",
            short(&local)
        ),
    ];
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(findings[0].target.as_deref().map(str::len), Some(40));
}

#[test]
fn the_patterns_are_options() {
    let mut repo = Repo::default();
    let activation = repo.commit(&[], "init", &[("a", Some("1"))]);
    let tip = repo.commit(
        &[&activation],
        "fix: thing [skip ci] NOCI",
        &[("a", Some("2"))],
    );
    repo.set_ref("refs/heads/main", &tip);

    let findings = repo
        .check(
            "no-ci-skip",
            &settings(&activation),
            r#"{"directive-pattern": "NOCI", "trailer-pattern": "^never$"}"#,
        )
        .expect("runs");
    assert_eq!(
        lines(&findings),
        [format!(
            " commit:{tip} {} carries the forbidden CI-skip directive NOCI",
            short(&tip)
        )]
    );

    let error = repo
        .check(
            "no-ci-skip",
            &settings(&activation),
            r#"{"directive-pattern": "("}"#,
        )
        .expect_err("invalid");
    assert!(
        error
            .message()
            .contains("directive-pattern \"(\" is not a valid regular expression"),
        "{error}"
    );
    assert!(error.is_config(), "a configuration error: {error}");

    // Read as JavaScript reads it: `(?i)` is no flag there, and it is refused by name.
    let error = repo
        .check(
            "no-ci-skip",
            &settings(&activation),
            r#"{"directive-pattern": "(?i)\\[skip ci\\]"}"#,
        )
        .expect_err("an inline flag");
    assert!(
        error
            .message()
            .contains("(? other than (?: and (?<name>, which the git plugin does not read"),
        "{error}"
    );
    assert!(error.is_config(), "a configuration error: {error}");
}

#[test]
fn only_a_commit_of_a_release_form_is_the_release() {
    let mut repo = Repo::default();
    let activation = repo.commit(&[], "init", &[("a", Some("1"))]);
    let release = repo.commit(&[&activation], RELEASE, &[("CHANGELOG.md", Some("1.2.0"))]);
    repo.set_ref("refs/heads/main", &release);
    let reported = [format!(
        " commit:{release} {} carries the forbidden CI-skip directive [skip ci]",
        short(&release)
    )];
    let found = |forms: &str| {
        lines(
            &repo
                .check("no-ci-skip", &merged(&settings(&activation), forms), "{}")
                .expect("runs"),
        )
    };

    // The default forms have a release form; so does a policy that lists one.
    assert_eq!(found("{}"), Vec::<String>::new());
    assert_eq!(
        found(
            r#"{"forms": [{"kind": "release", "parents": 1}, {"kind": "direct", "parents": 1}]}"#
        ),
        Vec::<String>::new()
    );
    // Without a release form, or when an earlier form takes the commit, it is not the release.
    assert_eq!(
        found(r#"{"forms": [{"kind": "direct", "parents": 1, "checks": []}]}"#),
        reported
    );
    assert_eq!(
        found(
            r#"{"forms": [{"kind": "direct", "parents": 1, "subject": "^chore", "checks": []},
                          {"kind": "release", "parents": 1}]}"#
        ),
        reported
    );
}

#[test]
fn the_message_being_written_is_checked_too() {
    let mut repo = Repo::default();
    let activation = repo.commit(&[], "init", &[("a", Some("1"))]);
    repo.set_ref("refs/heads/main", &activation);
    let mut check = |message: &str| {
        repo.set_hook(true);
        repo.set_message(Some(message));
        repo.check("no-ci-skip", &settings(&activation), "{}")
            .expect("runs")
    };

    let found = check("feat(core): add the thing [skip ci]\n\nThe body.\n");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].message,
        "the commit message carries the forbidden CI-skip directive [skip ci]"
    );
    assert_eq!(found[0].location, None);
    assert_eq!(
        found[0].target.as_deref(),
        Some("feat(core): add the thing [skip ci]")
    );
    assert_eq!(check("docs: note\n\nSkip-checks: true\n").len(), 1);

    // A `#` line counts: git keeps it in a message given with -m or -F. What follows the
    // scissors line is not part of the message, and the release commit carries one by design.
    assert_eq!(check("feat(core): add x\n\n#12 done [skip ci]").len(), 1);
    assert_eq!(
        check(
            "fix(core): mend it\n# ------------------------ >8 ------------------------\n+[skip ci]\n"
        )
        .len(),
        0
    );
    assert_eq!(check(RELEASE).len(), 0);
    assert_eq!(check("feat(core): add the thing\n").len(), 0);
}

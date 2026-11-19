use crate::repo::{Repo, lines};

const SETTINGS: &str = r#"{"main": ["refs/remotes/origin/main", "refs/heads/main"]}"#;
const OPTIONS: &str =
    r#"{"pattern": "^(feat|fix|docs)/[a-z0-9-]+$", "min-length": 8, "max-length": 20}"#;

/// A repository with one commit, checked out on `branch` (`None`: detached).
fn on(branch: Option<&str>) -> (Repo, String) {
    let mut repo = Repo::default();
    let head = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_head(&head);
    repo.set_branch(branch);
    (repo, head)
}

fn found(repo: &Repo, options: &str) -> Vec<String> {
    let findings = repo.check("branch-name", SETTINGS, options).expect("runs");
    for finding in &findings {
        assert!(finding.help.is_some());
    }
    lines(&findings)
}

#[test]
fn an_allowed_name_main_and_a_detached_head_pass() {
    for branch in [Some("feat/new-work"), Some("main"), None] {
        let (repo, _) = on(branch);
        assert_eq!(found(&repo, OPTIONS), Vec::<String>::new(), "{branch:?}");
    }
    // A repository without commits has nothing to point a finding at.
    let mut empty = Repo::default();
    empty.set_branch(Some("wip"));
    assert_eq!(found(&empty, OPTIONS), Vec::<String>::new());
}

#[test]
fn a_name_outside_the_pattern_or_its_length_is_reported() {
    let (repo, head) = on(Some("wip"));
    let findings = repo.check("branch-name", SETTINGS, OPTIONS).expect("runs");
    assert_eq!(
        lines(&findings),
        [
            format!(
                " commit:{head} branch wip does not match the pattern \"^(feat|fix|docs)/[a-z0-9-]+$\""
            ),
            format!(" commit:{head} branch wip is 3 characters long, shorter than min-length 8"),
        ]
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.target.as_deref() == Some("wip"))
    );

    let (repo, head) = on(Some("feat/a-very-long-branch-name"));
    assert_eq!(
        found(&repo, OPTIONS),
        [format!(
            " commit:{head} branch feat/a-very-long-branch-name is 28 characters long, longer than max-length 20"
        )]
    );

    // Without length bounds only the pattern counts.
    let (repo, head) = on(Some("feat/x"));
    assert_eq!(
        found(&repo, r#"{"pattern": "^feat/x$"}"#),
        Vec::<String>::new()
    );
    assert_eq!(
        found(&repo, r#"{"pattern": "^fix/"}"#),
        [format!(
            " commit:{head} branch feat/x does not match the pattern \"^fix/\""
        )]
    );
}

#[test]
fn the_options_are_checked() {
    let (repo, _) = on(Some("feat/new-work"));
    for (options, expected) in [
        ("{}", "missing field `pattern`"),
        (
            r#"{"pattern": "("}"#,
            "pattern \"(\" is not a valid regular expression",
        ),
        (
            r#"{"pattern": "^feat/", "min-length": 9, "max-length": 8}"#,
            "min-length 9 is above max-length 8",
        ),
        (
            r#"{"pattern": "^feat/", "length": 8}"#,
            "unknown field `length`",
        ),
    ] {
        let error = repo
            .check("branch-name", SETTINGS, options)
            .expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
    }
}

#[test]
fn a_push_checks_the_branch_it_updates() {
    let (mut repo, head) = on(Some("feat/new-work"));
    let pushed = repo.commit(&[&head], "more", &[("b", Some("b"))]);
    let found_at = |repo: &Repo| found(repo, OPTIONS);

    // `git push origin HEAD:bad` from a well-named branch: the pushed name counts.
    repo.set_range(&pushed, Some("refs/heads/bad"));
    assert_eq!(
        found_at(&repo),
        [
            format!(
                " commit:{pushed} branch bad does not match the pattern \"^(feat|fix|docs)/[a-z0-9-]+$\""
            ),
            format!(" commit:{pushed} branch bad is 3 characters long, shorter than min-length 8"),
        ]
    );
    // A push to main or to a tag updates no branch the rule names.
    for target in ["refs/heads/main", "refs/tags/v1.0.0"] {
        repo.set_range(&pushed, Some(target));
        assert_eq!(found_at(&repo), Vec::<String>::new(), "{target}");
    }
    // A well-named branch pushed from a badly named one passes.
    repo.set_branch(Some("wip"));
    repo.set_range(&pushed, Some("refs/heads/fix/the-bug"));
    assert_eq!(found_at(&repo), Vec::<String>::new());
    // A `--rev` range updates no ref: the checked-out branch counts.
    repo.set_range(&pushed, None);
    assert_eq!(found_at(&repo).len(), 2);
}

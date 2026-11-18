use crate::repo::{Repo, lines};

const SETTINGS: &str = r#"{"main": ["refs/remotes/origin/main", "refs/heads/main"]}"#;
const OPTIONS: &str =
    r#"{"pattern": "^(feat|fix|docs)/[a-z0-9-]+$", "min-length": 8, "max-length": 20}"#;
/// The reference's default policy: the template alone, every other option at its default.
const DEFAULT: &str = r#"{"template": ":type/:ticket-:name"}"#;

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

/// The findings for `branch` checked out, in a hook run or not.
fn verdict(branch: &str, options: &str, hook: bool) -> Vec<String> {
    let (mut repo, _) = on(Some(branch));
    repo.set_hook(hook);
    let findings = repo.check("branch-name", SETTINGS, options).expect("runs");
    findings
        .iter()
        .map(|finding| finding.message.clone())
        .collect()
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
fn a_pattern_is_searched_and_the_first_failing_check_is_reported() {
    let (repo, head) = on(Some("wip"));
    let findings = repo.check("branch-name", SETTINGS, OPTIONS).expect("runs");
    // Too short and outside the pattern: the length comes first, as in the reference.
    assert_eq!(
        lines(&findings),
        [format!(
            " commit:{head} branch wip is 3 characters long, shorter than min-length 8"
        )]
    );
    assert_eq!(findings[0].target.as_deref(), Some("wip"));

    let (repo, head) = on(Some("feat/a-very-long-branch-name"));
    assert_eq!(
        found(&repo, OPTIONS),
        [format!(
            " commit:{head} branch feat/a-very-long-branch-name is 28 characters long, longer than max-length 20"
        )]
    );

    // A pattern is searched anywhere in the name unless anchored.
    let (repo, head) = on(Some("chore/x-y"));
    assert_eq!(found(&repo, r#"{"pattern": "x"}"#), Vec::<String>::new());
    assert_eq!(
        found(&repo, r#"{"pattern": "^fix/"}"#),
        [format!(
            " commit:{head} branch chore/x-y does not match the pattern \"^fix/\""
        )]
    );
}

#[test]
fn the_default_template_takes_a_type_an_optional_ticket_and_a_name() {
    let fails = |branch: &str| {
        [format!(
            "branch {branch} does not match the template \":type/:ticket-:name\"; valid branch types: bugfix, feature, hotfix, release, support"
        )]
    };
    for branch in [
        "feature/proj-123-login-form",
        "feature/login-form",
        "bugfix/ab-1-x",
        "support/a",
        "hotfix/2026-fix",
    ] {
        assert_eq!(
            verdict(branch, DEFAULT, false),
            Vec::<String>::new(),
            "{branch}"
        );
    }
    for branch in [
        // A type not in `types`.
        "feat/login-form",
        // The ticket's letters are lower case; upper case is no free name either.
        "feature/PROJ-123-login",
        // A name with characters the subject does not take.
        "feature/login_form",
        // No name.
        "feature/",
    ] {
        assert_eq!(verdict(branch, DEFAULT, false), fails(branch), "{branch}");
    }
    // The ticket's own pattern: without it, `proj-123-login` is still a valid free name.
    assert_eq!(
        verdict("feature/proj-123", DEFAULT, false),
        Vec::<String>::new(),
        "the ticket left out: proj-123 is the name"
    );
}

#[test]
fn types_subjects_and_optional_placeholders_follow_the_options() {
    // Own types, and a subject string for every free placeholder.
    let own =
        r#"{"template": ":type/:scope/:name", "types": ["feat", "fix"], "subject": "[a-z]+"}"#;
    assert_eq!(verdict("feat/api/login", own, false), Vec::<String>::new());
    assert_eq!(verdict("fix/api/login-form", own, false).len(), 1);
    assert_eq!(verdict("docs/api/login", own, false).len(), 1);
    // A subject table: each placeholder its own pattern, the others the default.
    let table = r#"{"template": ":type/:ticket-:name", "subject": {"ticket": "[A-Z]+-[0-9]+", "name": "[a-z]+"}}"#;
    assert_eq!(
        verdict("feature/PROJ-7-login", table, false),
        Vec::<String>::new()
    );
    assert_eq!(verdict("feature/login", table, false), Vec::<String>::new());
    assert_eq!(verdict("feature/proj-7-login", table, false).len(), 1);
    assert_eq!(verdict("feature/log-in", table, false).len(), 1);
    // A placeholder followed by `-` anywhere is optional; leaving it out collapses the delimiters.
    let middle = r#"{"template": ":type/:area-:ticket-:name", "min-length": 0}"#;
    for branch in [
        "feature/web-ab-1-x",
        "feature/web-x",
        "feature/ab-1-x",
        "feature/x",
    ] {
        assert_eq!(
            verdict(branch, middle, false),
            Vec::<String>::new(),
            "{branch}"
        );
    }
    // Delimiters left at an end by an optional placeholder are trimmed.
    let leading = r#"{"template": ":ticket-:name", "min-length": 0}"#;
    assert_eq!(verdict("ab-1-x", leading, false), Vec::<String>::new());
    assert_eq!(verdict("x", leading, false), Vec::<String>::new());
    // No types: `:type` takes the default subject.
    let untyped = r#"{"template": ":type/:name", "types": []}"#;
    assert_eq!(
        verdict("anything/goes", untyped, false),
        Vec::<String>::new()
    );
}

#[test]
fn prohibited_names_count_only_in_a_hook_run_and_ignore_skips_the_rest() {
    for name in ["main", "master", "release"] {
        assert_eq!(
            verdict(name, DEFAULT, true),
            [format!(
                "branch {name} is prohibited; prohibited branch names: main, master, release"
            )],
            "{name} in a hook run"
        );
    }
    // A plain or CI run: main passes as main; master and release meet the length and the
    // template.
    assert_eq!(verdict("main", DEFAULT, false), Vec::<String>::new());
    assert_eq!(
        verdict("master", DEFAULT, false),
        [
            "branch master does not match the template \":type/:ticket-:name\"; valid branch types: bugfix, feature, hotfix, release, support"
        ]
    );
    // `dev` is ignored by default: too short and outside the template, it passes, hook or not.
    assert_eq!(verdict("dev", DEFAULT, true), Vec::<String>::new());
    assert_eq!(verdict("dev", DEFAULT, false), Vec::<String>::new());
    // An ignored name that is also prohibited is still refused in a hook run.
    let both = r#"{"template": ":type/:name", "ignore": ["main"]}"#;
    assert_eq!(verdict("main", both, true).len(), 1);
    // Outside a hook main passes; in one, with nothing prohibited, it is checked like any name.
    let open = r#"{"template": ":type/:name", "prohibited": []}"#;
    assert_eq!(verdict("main", open, false), Vec::<String>::new());
    assert_eq!(
        verdict("main", open, true),
        ["branch main is 4 characters long, shorter than min-length 5"]
    );
}

#[test]
fn both_length_limits_apply_and_zero_turns_one_off() {
    assert_eq!(
        verdict("a/b", DEFAULT, false),
        ["branch a/b is 3 characters long, shorter than min-length 5"]
    );
    let long = format!("feature/{}", "x".repeat(43));
    assert_eq!(
        verdict(&long, DEFAULT, false),
        [format!(
            "branch {long} is 51 characters long, longer than max-length 50"
        )]
    );
    let unbounded = r#"{"template": ":type/:name", "min-length": 0, "max-length": 0}"#;
    assert_eq!(verdict(&long, unbounded, false), Vec::<String>::new());
    assert_eq!(
        verdict(
            "support/a",
            r#"{"template": ":type/:name", "min-length": 10}"#,
            false
        ),
        ["branch support/a is 9 characters long, shorter than min-length 10"]
    );
}

#[test]
fn the_options_are_checked() {
    let (repo, _) = on(Some("feat/new-work"));
    for (options, expected) in [
        ("{}", "takes exactly one of template and pattern"),
        (
            r#"{"template": ":type/:name", "pattern": "^feat/"}"#,
            "takes exactly one of template and pattern",
        ),
        (
            r#"{"pattern": "("}"#,
            "pattern \"(\" is not a valid regular expression",
        ),
        (
            r#"{"template": ":type/:name", "subject": "("}"#,
            "subject \"(\" is not a valid regular expression",
        ),
        (
            r#"{"template": ":type/:name", "subject": {"name": "["}}"#,
            "subject.name \"[\" is not a valid regular expression",
        ),
        (
            r#"{"template": "-/-"}"#,
            "template \"-/-\" has nothing to match",
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
fn the_schema_refuses_a_template_and_a_pattern_together_at_load() {
    let info = gate_sdk::describe(&git::PLUGIN);
    let rule = info
        .rules
        .iter()
        .find(|rule| rule.name == "branch-name")
        .expect("the rule");
    let schema: serde_json::Value = serde_json::from_str(&rule.options_schema).expect("json");
    let validator = jsonschema::validator_for(&schema).expect("a schema");
    for (options, valid) in [
        (r#"{"template": ":type/:name"}"#, true),
        (r#"{"pattern": "^feat/"}"#, true),
        (r#"{"template": ":type/:name", "pattern": "^feat/"}"#, false),
        ("{}", false),
        (
            r#"{"template": ":type/:name", "subject": {"name": "[a-z]+"}}"#,
            true,
        ),
        (
            r#"{"template": ":type/:name", "prohibited": "main"}"#,
            false,
        ),
    ] {
        let value: serde_json::Value = serde_json::from_str(options).expect("json");
        assert_eq!(validator.is_valid(&value), valid, "{options}");
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
        [format!(
            " commit:{pushed} branch bad is 3 characters long, shorter than min-length 8"
        )]
    );
    // A push to main or to a tag updates no branch the rule names.
    for target in ["refs/heads/main", "refs/tags/v1.0.0"] {
        repo.set_range(&pushed, Some(target));
        assert_eq!(found_at(&repo), Vec::<String>::new(), "{target}");
    }
    // A hook's push to main is refused when main is prohibited.
    repo.set_hook(true);
    repo.set_range(&pushed, Some("refs/heads/main"));
    assert_eq!(found(&repo, DEFAULT).len(), 1);
    repo.set_hook(false);
    // A well-named branch pushed from a badly named one passes.
    repo.set_branch(Some("wip"));
    repo.set_range(&pushed, Some("refs/heads/fix/the-bug"));
    assert_eq!(found_at(&repo), Vec::<String>::new());
    // A `--rev` range updates no ref: the checked-out branch counts.
    repo.set_range(&pushed, None);
    assert_eq!(found_at(&repo).len(), 1);
}

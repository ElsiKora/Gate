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
    // Lengths are JavaScript's, in UTF-16 code units: an emoji counts twice.
    let emoji = r#"{"pattern": ".", "min-length": 0, "max-length": 5}"#;
    assert_eq!(verdict("ab\u{1F600}c", emoji, false), Vec::<String>::new());
    assert_eq!(
        verdict("ab\u{1F600}cd", emoji, false),
        ["branch ab\u{1F600}cd is 6 characters long, longer than max-length 5"]
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
            "subject.name \"[\" has a [ whose class is not closed",
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
        assert!(error.is_config(), "a configuration error: {options}");
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

#[test]
fn word_and_digit_classes_are_ascii_as_in_the_reference() {
    // JavaScript without the `u` flag: `\w` is [0-9A-Za-z_], so `é` is not a word character.
    let words = r#"{"template": ":type/:ticket-:name", "subject": "[\\w-]+"}"#;
    assert_eq!(
        verdict("feature/proj-1-cafe", words, false),
        Vec::<String>::new()
    );
    assert_eq!(verdict("feature/proj-1-café", words, false).len(), 1);
    let pattern = r#"{"pattern": "^feature/\\w+-\\d+$"}"#;
    assert_eq!(
        verdict("feature/ab-12", pattern, false),
        Vec::<String>::new()
    );
    assert_eq!(verdict("feature/äb-12", pattern, false).len(), 1);
    assert_eq!(verdict("feature/ab-١٢", pattern, false).len(), 1);
}

/// Whether `branch`, checked out outside a hook, passes `pattern` with no length limits.
fn passes(pattern: &str, branch: &str) -> bool {
    let options = gate_sdk::serde_json::json!({
        "pattern": pattern,
        "min-length": 0,
        "max-length": 0,
    });
    verdict(branch, &options.to_string(), false).is_empty()
}

// Each case below is regex-crate syntax that JavaScript reads as characters; a pattern means what
// git-branch-lint means by it.

#[test]
fn angle_bracket_escapes_are_characters_not_word_boundaries() {
    assert!(passes(r"^\<a\>$", "<a>"));
    assert!(!passes(r"^\<a\>$", "a"));
}

#[test]
fn backslash_a_and_z_escapes_are_letters_not_assertions() {
    assert!(passes(r"^\Afeat\z$", "Afeatz"));
    assert!(!passes(r"^\Afeat\z$", "feat"));
}

#[test]
fn backslash_p_escapes_are_letters_not_unicode_classes() {
    assert!(passes(r"^feat/\pL$", "feat/pL"));
    assert!(!passes(r"^feat/\pL$", "feat/a"));
}

#[test]
fn x_escapes_without_two_hex_digits_are_the_letter_x() {
    // `{2}` after it is then a quantifier of `x`.
    assert!(passes(r"^feat/\x{2}$", "feat/xx"));
    assert!(!passes(r"^feat/\x{2}$", "feat/\u{2}"));
}

#[test]
fn a_brace_that_makes_no_quantifier_is_a_character() {
    assert!(passes(r"^feat/a{,2}$", "feat/a{,2}"));
    assert!(!passes(r"^feat/a{,2}$", "feat/a"));
}

#[test]
fn a_posix_class_is_characters_of_a_class() {
    assert!(passes(r"^feat/[[:alpha:]]$", "feat/:]"));
    assert!(!passes(r"^feat/[[:alpha:]]$", "feat/b"));
}

#[test]
fn a_nested_bracket_is_a_character_of_the_class() {
    assert!(passes(r"^feat/[[a]]$", "feat/[]"));
    assert!(!passes(r"^feat/[[a]]$", "feat/a"));
}

#[test]
fn a_double_ampersand_in_a_class_is_characters_not_an_intersection() {
    assert!(passes(r"^feat/[a&&b]$", "feat/&"));
    assert!(passes(r"^feat/[a&&b]$", "feat/a"));
}

#[test]
fn a_double_tilde_in_a_class_is_characters_not_a_symmetric_difference() {
    assert!(passes(r"^feat/[a~~b]$", "feat/~"));
}

#[test]
fn a_double_dash_in_a_class_is_a_range_not_a_difference() {
    assert!(passes(r"^feat/[%--]$", "feat/+"));
    assert!(!passes(r"^feat/[%--]$", "feat/a"));
}

#[test]
fn dot_and_space_are_javascripts() {
    // `.` matches no line terminator; `\s` takes U+FEFF and not U+0085.
    assert!(!passes(r"^feat/.$", "feat/\u{2028}"));
    assert!(passes(r"^feat/\s$", "feat/\u{feff}"));
    assert!(!passes(r"^feat/\s$", "feat/\u{85}"));
}

/// What `git/branch-name` judges in `repo` with the exceptions `excepted`, and the targets of its
/// findings.
fn judged(repo: &Repo, excepted: &[&str]) -> (Vec<String>, Vec<String>) {
    let output = repo
        .check_excepted("branch-name", SETTINGS, OPTIONS, excepted)
        .expect("runs");
    let [judged] = output.judged.as_slice() else {
        panic!(
            "the rule narrows its view in every run: {:?}",
            output.judged
        );
    };
    let targets = output
        .findings
        .iter()
        .map(|finding| finding.target.clone().unwrap_or_default())
        .collect();
    (judged.targets.clone(), targets)
}

#[test]
fn a_run_judges_its_branch_and_every_excepted_name_by_a_hook_s_verdict() {
    let owned =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    let (repo, _) = on(Some("feat/new-work"));
    // Outside a hook too, `main` is judged as a hook judges it: prohibited, so its exception
    // accepts a finding in every run; `wip` is too short; `feat/ok-name` passes everywhere, so
    // its exception is stale.
    assert_eq!(
        judged(&repo, &["main", "wip", "feat/ok-name"]),
        (
            owned(&["feat/new-work", "main", "wip", "feat/ok-name"]),
            owned(&["main", "wip"])
        )
    );
    // The exceptions of other branches are not due on this one.
    assert_eq!(judged(&repo, &[]), (owned(&["feat/new-work"]), Vec::new()));

    // On main outside a hook: main passes, unless an exception names it.
    let (main, _) = on(Some("main"));
    assert_eq!(judged(&main, &[]), (owned(&["main"]), Vec::new()));
    assert_eq!(
        judged(&main, &["main"]),
        (owned(&["main"]), owned(&["main"]))
    );

    // A detached HEAD judges the excepted names only.
    let (detached, _) = on(None);
    assert_eq!(
        judged(&detached, &["wip"]),
        (owned(&["wip"]), owned(&["wip"]))
    );
}

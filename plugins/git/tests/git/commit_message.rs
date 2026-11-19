use gate_sdk::Finding;

use crate::repo::Repo;

/// The aspects `git/commit-message` reports for `message` with `options`.
fn aspects(message: Option<&str>, options: &str) -> Vec<String> {
    findings(message, options)
        .iter()
        .map(|finding| finding.aspect.clone())
        .collect()
}

fn findings(message: Option<&str>, options: &str) -> Vec<Finding> {
    let mut repo = Repo::default();
    let root = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &root);
    repo.set_hook(true);
    repo.set_message(message);
    repo.check("commit-message", "{}", options).expect("runs")
}

const GOOD: &str = "feat(core): add the thing\n\nThe body says why.\n\nCloses #12\nBREAKING CHANGE: the old call is gone\n";

#[test]
fn a_conventional_message_passes_every_check() {
    assert_eq!(aspects(Some(GOOD), "{}"), Vec::<String>::new());
    let limits = [
        // header at 100, subject at 80, scope at 30
        format!("refactor({}): {}", "s".repeat(8), "a".repeat(80)),
        format!("feat({}): add the thing", "a".repeat(30)),
        // subject at 3, several scopes, a quoted capital, an ellipsis is no full stop
        "feat(core): add".to_owned(),
        "feat(core, api/web): add the thing".to_owned(),
        "feat(core): add the `README` file".to_owned(),
        // a long body line with a URL
        format!(
            "fix(core): mend the link\n\nSee https://example.test/{} for it.",
            "p".repeat(100)
        ),
    ];
    for message in &limits {
        assert_eq!(
            aspects(Some(message), "{}"),
            Vec::<String>::new(),
            "{message}"
        );
    }
}

#[test]
fn each_header_type_and_scope_check_fails_as_the_reference_fails_it() {
    let cases: Vec<(String, &str, Vec<&str>)> = vec![
        (
            "feat(core): add the thing ".to_owned(),
            "{}",
            vec!["header-trim"],
        ),
        (
            format!("refactor({}): {}", "s".repeat(15), "a".repeat(80)),
            "{}",
            vec!["header-max-length"],
        ),
        (
            "fix(core): add x y".to_owned(),
            r#"{"header-min-length": 20}"#,
            vec!["header-min-length"],
        ),
        (
            "feat(core): add the Thing".to_owned(),
            "{}",
            vec!["header-case", "subject-case"],
        ),
        (
            "feat(core): add the thing.".to_owned(),
            "{}",
            vec!["header-full-stop", "subject-full-stop"],
        ),
        (
            "feat(core): add the thing...".to_owned(),
            "{}",
            vec!["header-full-stop"],
        ),
        ("(core): add the thing".to_owned(), "{}", vec!["type-empty"]),
        (
            "FEAT(core): add the thing".to_owned(),
            "{}",
            vec!["header-case", "type-case", "type-enum"],
        ),
        (
            "feature(core): add the thing".to_owned(),
            "{}",
            vec!["type-enum"],
        ),
        ("feat: add the thing".to_owned(), "{}", vec!["scope-empty"]),
        (
            "feat(Core): add the thing".to_owned(),
            "{}",
            vec!["header-case", "scope-case"],
        ),
        (
            format!("feat({}): add the thing", "a".repeat(31)),
            "{}",
            vec!["scope-max-length"],
        ),
    ];
    for (message, options, expected) in &cases {
        assert_eq!(aspects(Some(message), options), *expected, "{message:?}");
    }
}

#[test]
fn each_subject_body_and_footer_check_fails_as_the_reference_fails_it() {
    let long_body = format!("{}.", "b".repeat(100));
    let cases: Vec<(String, &str, Vec<&str>)> = vec![
        (
            "feat(core): ".to_owned(),
            "{}",
            vec!["header-trim", "subject-empty"],
        ),
        (
            "feat(core) add the thing".to_owned(),
            "{}",
            vec!["type-empty", "scope-empty", "subject-empty"],
        ),
        (
            "feat(core): ab".to_owned(),
            "{}",
            vec!["subject-min-length"],
        ),
        (
            format!("feat(core): {}", "a".repeat(81)),
            "{}",
            vec!["subject-max-length"],
        ),
        // JavaScript's length: the emoji is two UTF-16 code units.
        (
            format!("feat(core): {}\u{1F600}", "a".repeat(79)),
            "{}",
            vec!["subject-max-length"],
        ),
        (
            "feat(core): add the thing\nThe body.".to_owned(),
            "{}",
            vec!["body-leading-blank"],
        ),
        (
            format!("feat(core): add the thing\n\n{long_body}"),
            "{}",
            vec!["body-max-line-length"],
        ),
        (
            "feat(core): add the thing\n\nThe body".to_owned(),
            "{}",
            vec!["body-full-stop"],
        ),
        (
            "feat(core): add the thing\n\nThe body.\nCloses #12".to_owned(),
            "{}",
            vec!["footer-leading-blank"],
        ),
        (
            format!(
                "feat(core): add the thing\n\nThe body.\n\nRefs #1 {}",
                "f".repeat(100)
            ),
            "{}",
            vec!["footer-max-line-length"],
        ),
        (
            "fix(core): add the thing".to_owned(),
            r#"{"types": ["feat"]}"#,
            vec!["type-enum"],
        ),
    ];
    for (message, options, expected) in &cases {
        assert_eq!(aspects(Some(message), options), *expected, "{message:?}");
    }
}

#[test]
fn the_findings_say_what_is_wrong_and_name_the_header() {
    let found = findings(Some("feature(core): add the thing"), "{}");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].message,
        "type feature is not a commit type; the commit types: feat, fix, docs, style, refactor, \
         perf, test, build, ci, chore, revert, wip"
    );
    assert_eq!(
        found[0].target.as_deref(),
        Some("feature(core): add the thing")
    );
    assert_eq!(found[0].location, None);

    let short = findings(Some("ci(a): x"), "{}");
    let messages: Vec<(&str, &str)> = short
        .iter()
        .map(|finding| (finding.aspect.as_str(), finding.message.as_str()))
        .collect();
    assert_eq!(
        messages,
        [
            (
                "header-min-length",
                "the header is 8 characters long, shorter than header-min-length 10"
            ),
            (
                "subject-min-length",
                "the subject is 1 characters long, shorter than subject-min-length 3"
            ),
        ]
    );
}

#[test]
fn the_warning_checks_are_aspects_of_their_own() {
    let info = gate_sdk::describe(&git::PLUGIN);
    let rule = info
        .rules
        .iter()
        .find(|rule| rule.name == "commit-message")
        .expect("declared");
    for aspect in ["header-min-length", "scope-max-length"] {
        assert!(rule.aspects.iter().any(|known| known == aspect), "{aspect}");
    }
    assert_eq!(rule.aspects.len(), 21);
    assert_eq!(
        aspects(Some(&format!("feat({}): add x", "a".repeat(31))), "{}"),
        ["scope-max-length"]
    );
}

#[test]
fn comment_lines_and_the_scissors_section_are_not_part_of_the_message() {
    let edited = format!(
        "feat(core): add the thing\n\nThe body says why.\n\n# Please enter the commit message for your changes.\n# {}\n#\n# On branch feature/x\n",
        "c".repeat(120)
    );
    assert_eq!(aspects(Some(&edited), "{}"), Vec::<String>::new());

    let verbose = format!(
        "fix(core): mend the thing\n# ------------------------ >8 ------------------------\n# Do not modify or remove the line above.\ndiff --git a/x b/x\n+{}\n",
        "d".repeat(120)
    );
    assert_eq!(aspects(Some(&verbose), "{}"), Vec::<String>::new());

    // Without the comment filter, the second line would not be blank.
    let commented = "feat(core): add the thing\n# a note\n\nThe body.";
    assert_eq!(aspects(Some(commented), "{}"), ["body-leading-blank"]);

    assert_eq!(
        aspects(Some("# only a comment\n# and another\n"), "{}"),
        Vec::<String>::new()
    );
}

#[test]
fn a_run_without_a_message_or_with_a_blank_one_checks_nothing() {
    assert_eq!(aspects(None, "{}"), Vec::<String>::new());
    assert_eq!(aspects(Some(""), "{}"), Vec::<String>::new());
    assert_eq!(aspects(Some(" \n\n"), "{}"), Vec::<String>::new());
}

#[test]
fn merges_reverts_fixups_and_releases_are_not_checked() {
    for message in [
        "Merge branch 'feature/x' into main",
        "Merge pull request #3 from someone/branch",
        "Merge tag 'v1.0.0'",
        "Merge remote-tracking branch 'origin/main'",
        "Revert \"feat(core): add the thing\"\n\nThis reverts commit abc.",
        "fixup! feat(core): add the thing",
        "squash! feat(core): add the thing",
        "amend! feat(core): add the thing",
        "chore(release): 1.2.0 [skip ci]",
        "v1.2.3",
        "Merged in feature/x (pull request #3)",
        "Automatic merge from release",
        "Auto-merged one into two",
    ] {
        assert_eq!(
            aspects(Some(message), "{}"),
            Vec::<String>::new(),
            "{message}"
        );
    }
    // Close to an ignored form, but not one.
    assert_eq!(
        aspects(Some("Merging things in"), "{}"),
        ["header-case", "type-empty", "scope-empty", "subject-empty"]
    );
    assert_eq!(
        aspects(Some("chore(Release): 1.2"), "{}"),
        ["header-case", "scope-case"]
    );
    assert_eq!(
        aspects(Some("chore(Release): 1.2.0"), "{}"),
        Vec::<String>::new()
    );
}

#[test]
fn the_options_are_checked() {
    let mut repo = Repo::default();
    let root = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &root);
    repo.set_message(Some(GOOD));
    let error = repo
        .check("commit-message", "{}", r#"{"type-enum": ["feat"]}"#)
        .expect_err("unknown option");
    assert!(
        error.message().contains("unknown field `type-enum`"),
        "{error}"
    );
}

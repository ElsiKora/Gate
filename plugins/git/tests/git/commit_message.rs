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
    assert_eq!(rule.aspects.len(), 23);
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

/// A policy written as one header expression: eleven types, an optional lower-case scope or
/// comma-separated scopes of `[a-z0-9-]`, an optional `!`, then `: ` and a subject. Merges and
/// `Revert "..."` are skipped by its own ignores, and nothing else is.
const HEADER_POLICY: &str = r#"{
    "types": ["build", "chore", "ci", "docs", "feat", "fix", "perf", "refactor", "revert", "style", "test"],
    "header-pattern": "^(?<type>\\w*)(?:\\((?<scope>[a-z0-9-]+(?:,[a-z0-9-]+)*)\\))?!?: (?<subject>.+)$",
    "ignores": ["^Merge ", "^Revert \""],
    "default-ignores": false,
    "header-max-length": 0, "header-min-length": 0, "scope-max-length": 0,
    "subject-max-length": 0, "subject-min-length": 0,
    "body-max-line-length": 0, "footer-max-line-length": 0
}"#;

/// The aspects the policy keeps on; it turns the others off with `levels`.
const POLICY_ASPECTS: &[&str] = &[
    "header-trim",
    "header-max-length",
    "header-min-length",
    "type-empty",
    "type-case",
    "type-enum",
    "scope-case",
    "subject-empty",
];

/// Whether `message` passes the policy.
fn passes_policy(message: &str) -> bool {
    aspects(Some(message), HEADER_POLICY)
        .iter()
        .all(|aspect| !POLICY_ASPECTS.contains(&aspect.as_str()))
}

#[test]
fn a_header_pattern_with_its_own_ignores_gives_the_policy_s_verdicts() {
    let long = format!("feat(a,b,c): {}", "word ".repeat(30).trim_end());
    let cases: [(&str, bool); 30] = [
        ("feat: add x", true),
        ("feat(core): add x", true),
        ("feat(a,b): add x", true),
        ("feat(a-1)!: add x", true),
        ("refactor!: drop x", true),
        (long.as_str(), true),
        ("feat: Add X.", true),
        ("feat: x\nbody without a blank line", true),
        ("# a comment\nfeat: add x", true),
        ("\n\nfeat: add x", true),
        ("Merge branch 'x'", true),
        ("Merge tag 'v1'", true),
        ("Merge commit 'abc'", true),
        ("Merge x", true),
        ("Revert \"feat: x\"", true),
        ("chore(release): 1.2.3 and more", true),
        // The reference reads the blank line after the comment as the header, as commitlint
        // does; a policy that skips blank lines would pass it.
        ("# a comment\n\nfeat: add x", false),
        ("Feat: add x", false),
        ("wip: add x", false),
        ("feat(Core): add x", false),
        ("feat:add x", false),
        ("feat: ", false),
        (" feat: add x", false),
        ("feat(a_b): add x", false),
        ("feat(a/b): add x", false),
        ("feat(a b): add x", false),
        ("feat(): add x", false),
        ("fixup! feat: x", false),
        ("squash! feat: x", false),
        ("Reapply \"feat: x\"", false),
    ];
    for (message, passes) in cases {
        assert_eq!(passes_policy(message), passes, "{message:?}");
    }
    // The header pattern alone decides the header, a breaking `!` included.
    assert!(passes_policy("feat(a)!: add x"));
    assert!(!passes_policy("feat(a_b)!: add x"));
}

#[test]
fn a_header_pattern_reads_word_characters_as_ascii() {
    // Only an ASCII `\w` refuses the scope `é`; a Unicode one would read it as a word.
    let word_scope =
        r#"{"header-pattern": "^(?<type>\\w+)(?:\\((?<scope>\\w+)\\))?: (?<subject>.+)$"}"#;
    let ascii = aspects(Some("feat(e): add the thing"), word_scope);
    assert!(!ascii.contains(&"type-empty".to_owned()), "{ascii:?}");
    let accented = aspects(Some("feat(\u{e9}): add the thing"), word_scope);
    assert!(accented.contains(&"type-empty".to_owned()), "{accented:?}");
}

#[test]
fn an_ignore_is_searched_in_the_whole_message_and_its_caret_anchors_at_the_start() {
    let message = "wip x\n\nWIP";
    let checked = aspects(Some(message), "{}");
    assert!(checked.contains(&"type-empty".to_owned()), "{checked:?}");
    // `WIP` in the body skips the message: the whole message is searched.
    assert_eq!(
        aspects(Some(message), r#"{"ignores": ["WIP"]}"#),
        Vec::<String>::new()
    );
    // `^` anchors at the start of the message, not at each line.
    let start = r#"{"ignores": ["^WIP"]}"#;
    assert_eq!(aspects(Some("WIP x"), start), Vec::<String>::new());
    assert_eq!(aspects(Some(message), start), checked);
}

#[test]
fn without_the_default_ignores_a_fixup_is_checked() {
    assert_eq!(aspects(Some("fixup! x"), "{}"), Vec::<String>::new());
    assert_eq!(
        aspects(Some("fixup! x"), r#"{"default-ignores": false}"#),
        [
            "header-min-length",
            "type-empty",
            "scope-empty",
            "subject-empty"
        ]
    );
    // The repository's own ignores are added to the defaults.
    assert_eq!(
        aspects(Some("Merge x"), r#"{"ignores": ["^Merge "]}"#),
        Vec::<String>::new()
    );
    assert_eq!(
        aspects(Some("fixup! x"), r#"{"ignores": ["^Merge "]}"#),
        Vec::<String>::new()
    );
}

#[test]
fn a_header_pattern_or_an_ignore_that_cannot_work_fails_closed() {
    let mut repo = Repo::default();
    let root = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &root);
    repo.set_message(Some(GOOD));
    for (options, expected) in [
        (
            r#"{"header-pattern": "("}"#,
            "git/commit-message header-pattern \"(\" is not a valid regular expression; write a \
             JavaScript regular expression without lookaround or backreferences, with the named \
             groups type, scope and subject",
        ),
        (
            r#"{"header-pattern": "^(?<kind>\\w+): (?<subject>.+)$"}"#,
            "git/commit-message header-pattern \"^(?<kind>\\\\w+): (?<subject>.+)$\" has no group \
             named type; write a JavaScript regular expression without lookaround",
        ),
        (
            r#"{"header-pattern": "^(\\w+): (.+)$"}"#,
            "has no group named type",
        ),
        (
            r#"{"ignores": ["["]}"#,
            "git/commit-message ignores \"[\" has a [ whose class is not closed, which the rule \
             does not read",
        ),
        (
            r#"{"header-pattern": "^(?<type>\\w+)(?=:): (?<subject>.+)$"}"#,
            "has a lookaround (?=, (?!, (?<= or (?<!, which the rule does not read",
        ),
        (
            r#"{"ignores": ["(a)\\1"]}"#,
            "has the backreference or octal escape \\1",
        ),
    ] {
        let error = repo
            .check("commit-message", "{}", options)
            .expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
        assert!(error.is_config(), "a configuration error: {options}");
    }
}

/// A policy that refuses an AI co-author trailer, in either case of its token.
const NO_AGENT: &str = r#"{"prohibited-trailers": ["^[Cc]o-[Aa]uthored-[Bb]y: .*\\b[Aa]gent\\b"]}"#;

/// The trailers `git/commit-message` refuses in `message` under `options`.
fn refused(message: &str, options: &str) -> Vec<String> {
    findings(Some(message), options)
        .into_iter()
        .filter(|finding| finding.aspect == "prohibited-trailers")
        .filter_map(|finding| finding.target)
        .collect()
}

#[test]
fn a_prohibited_trailer_in_the_trailer_block_fails_and_is_named() {
    let message = "feat(core): add the thing\n\nThe body says why.\n\nReviewed-by: A Person\nCo-Authored-By: Example Agent\n";
    let found: Vec<_> = findings(Some(message), NO_AGENT)
        .into_iter()
        .filter(|finding| finding.aspect == "prohibited-trailers")
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].message,
        "the trailer \"Co-Authored-By: Example Agent\" is prohibited: it matches \
         prohibited-trailers \"^[Cc]o-[Aa]uthored-[Bb]y: .*\\\\b[Aa]gent\\\\b\""
    );
    assert_eq!(
        found[0].target.as_deref(),
        Some("Co-Authored-By: Example Agent")
    );
    // Each refused trailer is a finding of its own, and an ignored message is checked too.
    let two = "fixup! feat(core): add the thing\n\nCo-authored-by: Example Agent\nCo-authored-by: Other Agent\n";
    assert_eq!(
        aspects(Some(two), NO_AGENT),
        ["prohibited-trailers", "prohibited-trailers"]
    );
    // A comment line in the block is not part of it: kept, it would be prose, and the block no
    // trailer block. A trailing old-style Conflicts: block is not part of the message, as in git.
    for message in [
        "feat(core): add the thing\n\nThe body says why.\n\nCo-authored-by: Example Agent\n# a note\nReviewed-by: A Person\n",
        "feat(core): add the thing\n\nThe body says why.\n\nCo-authored-by: Example Agent\n\nConflicts:\n\tsrc/a.rs\n\tsrc/b.rs\n",
    ] {
        assert_eq!(
            refused(message, NO_AGENT),
            ["Co-authored-by: Example Agent"],
            "{message:?}"
        );
    }
}

#[test]
fn the_same_text_outside_the_trailer_block_passes() {
    for message in [
        // A paragraph above the last.
        "feat(core): add the thing\n\nCo-authored-by: Example Agent\n\nThe body says why.\n",
        // The last paragraph, but with prose in it, so not a trailer block.
        "feat(core): add the thing\n\nThe body says why.\nCo-authored-by: Example Agent\n",
        // The title.
        "Co-authored-by: Example Agent\n",
    ] {
        assert_eq!(
            refused(message, NO_AGENT),
            Vec::<String>::new(),
            "{message:?}"
        );
    }
}

#[test]
fn a_trailer_no_pattern_matches_passes() {
    let message = "feat(core): add the thing\n\nThe body says why.\n\nSigned-off-by: A Person\nCo-authored-by: Another Person\nAgent: none\n";
    assert_eq!(refused(message, NO_AGENT), Vec::<String>::new());
    // Without the option nothing is refused.
    let agent =
        "feat(core): add the thing\n\nThe body says why.\n\nCo-authored-by: Example Agent\n";
    assert_eq!(refused(agent, "{}"), Vec::<String>::new());
    assert_eq!(refused(agent, NO_AGENT), ["Co-authored-by: Example Agent"]);
}

#[test]
fn a_prohibited_trailer_pattern_that_cannot_work_fails_closed() {
    let mut repo = Repo::default();
    let root = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &root);
    repo.set_message(Some(GOOD));
    for (options, expected) in [
        (
            r#"{"prohibited-trailers": ["("]}"#,
            "git/commit-message prohibited-trailers \"(\" is not a valid regular expression; \
             write a JavaScript regular expression without lookaround or backreferences that the \
             trailers to refuse match",
        ),
        (
            r#"{"prohibited-trailers": [""]}"#,
            "git/commit-message prohibited-trailers has an empty pattern, which refuses every \
             trailer",
        ),
        (
            r#"{"prohibited-trailers": ["^Co-authored-by: (?!Person)"]}"#,
            "git/commit-message prohibited-trailers \"^Co-authored-by: (?!Person)\" has a \
             lookaround",
        ),
    ] {
        let error = repo
            .check("commit-message", "{}", options)
            .expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
        assert!(error.is_config(), "a configuration error: {options}");
    }
}

/// A policy that refuses a body line saying the message was generated, in either case.
const NO_GENERATED: &str = r#"{"prohibited-body-lines": ["^[Gg]enerated (with|by) "]}"#;

/// The body lines `git/commit-message` refuses in `message` under `options`.
fn refused_lines(message: &str, options: &str) -> Vec<String> {
    findings(Some(message), options)
        .into_iter()
        .filter(|finding| finding.aspect == "prohibited-body-lines")
        .filter_map(|finding| finding.target)
        .collect()
}

#[test]
fn a_prohibited_body_line_fails_and_is_named() {
    let message = "feat(core): add the thing\n\nThe body says why.\nGenerated with an assistant.\n\nCo-authored-by: A Person\n";
    let found: Vec<_> = findings(Some(message), NO_GENERATED)
        .into_iter()
        .filter(|finding| finding.aspect == "prohibited-body-lines")
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].message,
        "the body line \"Generated with an assistant.\" is prohibited: it matches \
         prohibited-body-lines \"^[Gg]enerated (with|by) \""
    );
    assert_eq!(
        found[0].target.as_deref(),
        Some("Generated with an assistant.")
    );
    assert_eq!(
        found[0].help.as_deref(),
        Some("remove the line from the message")
    );
    // Each refused line is a finding of its own, in any paragraph, the second line of the
    // message included, and an ignored message is checked too.
    for (message, expected) in [
        (
            "feat(core): add the thing\nGenerated by a tool.\n\nThe body says why.\n\ngenerated with an assistant\n",
            vec!["Generated by a tool.", "generated with an assistant"],
        ),
        (
            "fixup! feat(core): add the thing\n\nGenerated with an assistant\n",
            vec!["Generated with an assistant"],
        ),
        (
            "Merge branch 'topic'\n\nGenerated with an assistant\n",
            vec!["Generated with an assistant"],
        ),
    ] {
        assert_eq!(
            refused_lines(message, NO_GENERATED),
            expected,
            "{message:?}"
        );
    }
}

#[test]
fn the_same_text_in_the_header_the_trailer_block_or_a_comment_passes() {
    let any = r#"{"prohibited-body-lines": ["[Gg]enerated"]}"#;
    for message in [
        // The header.
        "feat(core): generated with care\n\nThe body says why.\n",
        // The trailer block, which prohibited-trailers reads.
        "feat(core): add the thing\n\nThe body says why.\n\nGenerated-with: an assistant\n",
        // A comment line and the scissors section are not part of the message.
        "feat(core): add the thing\n\nThe body says why.\n# Generated with an assistant\n",
        "feat(core): add the thing\n\nThe body says why.\n# ------------------------ >8 ------------------------\nGenerated with an assistant\n",
    ] {
        assert_eq!(
            refused_lines(message, any),
            Vec::<String>::new(),
            "{message:?}"
        );
    }
    // The same block with a line of prose in it is no trailer block: every line is a body line.
    assert_eq!(
        refused_lines(
            "feat(core): add the thing\n\nThe body says why.\n\nGenerated-with: an assistant\nand more prose\n",
            any
        ),
        ["Generated-with: an assistant"]
    );
}

#[test]
fn prose_that_git_lets_a_trailer_block_hold_is_read_as_body_lines() {
    // With a line git writes itself, a quarter of trailers makes the paragraph a trailer block,
    // prose included; that prose, an indented line after it too, is still read.
    let anywhere = r#"{"prohibited-body-lines": ["[Gg]enerated (with|by) "]}"#;
    assert_eq!(
        refused_lines(
            "feat(core): add the thing\n\nThe body says why.\n\nGenerated with an assistant\nSigned-off-by: A Person\n",
            NO_GENERATED
        ),
        ["Generated with an assistant"]
    );
    assert_eq!(
        refused_lines(
            "feat(core): add the thing\n\nThe body says why.\n\nA note.\n  generated by a tool\nSigned-off-by: A Person\n",
            anywhere
        ),
        ["  generated by a tool"]
    );
    // The paragraph is still the trailer block: its trailer is read by prohibited-trailers.
    assert_eq!(
        refused(
            "feat(core): add the thing\n\nThe body says why.\n\nGenerated with an assistant\nSigned-off-by: A Person\n",
            r#"{"prohibited-trailers": ["^Signed-off-by: "]}"#
        ),
        ["Signed-off-by: A Person"]
    );
    // An indented line after a trailer continues it: it is part of that trailer, unfolded, and
    // prohibited-trailers reads it there.
    let continued = "feat(core): add the thing\n\nThe body says why.\n\nSigned-off-by: A Person\n  Generated with an assistant\n";
    assert_eq!(refused_lines(continued, anywhere), Vec::<String>::new());
    assert_eq!(
        refused(
            continued,
            r#"{"prohibited-trailers": ["[Gg]enerated (with|by) "]}"#
        ),
        ["Signed-off-by: A Person Generated with an assistant"]
    );
}

#[test]
fn a_body_line_no_pattern_matches_passes() {
    let message = "feat(core): add the thing\n\nThe body says why, and what was generated.\nA generator writes it with care.\n";
    assert_eq!(refused_lines(message, NO_GENERATED), Vec::<String>::new());
    // Without the option nothing is refused.
    let generated = "feat(core): add the thing\n\nGenerated with an assistant.\n";
    assert_eq!(refused_lines(generated, "{}"), Vec::<String>::new());
    assert_eq!(
        refused_lines(generated, NO_GENERATED),
        ["Generated with an assistant."]
    );
}

#[test]
fn a_prohibited_body_line_pattern_that_cannot_work_fails_closed() {
    let mut repo = Repo::default();
    let root = repo.commit(&[], "init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &root);
    repo.set_message(Some(GOOD));
    for (options, expected) in [
        (
            r#"{"prohibited-body-lines": ["("]}"#,
            "git/commit-message prohibited-body-lines \"(\" is not a valid regular expression; \
             write a JavaScript regular expression without lookaround or backreferences that the \
             body lines to refuse match",
        ),
        (
            r#"{"prohibited-body-lines": [""]}"#,
            "git/commit-message prohibited-body-lines has an empty pattern, which refuses every \
             body line",
        ),
        (
            r#"{"prohibited-body-lines": ["^Generated (?!by hand)"]}"#,
            "git/commit-message prohibited-body-lines \"^Generated (?!by hand)\" has a \
             lookaround",
        ),
    ] {
        let error = repo
            .check("commit-message", "{}", options)
            .expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
        assert!(error.is_config(), "a configuration error: {options}");
    }
}

#[test]
fn body_full_stop_judges_the_body_above_the_trailer_block() {
    for (message, expected) in [
        // A full stop above a person's co-author passes.
        (
            "feat(core): add the thing\n\nThe body says why.\n\nCo-authored-by: A Person\n",
            Vec::<&str>::new(),
        ),
        // A body that is only a trailer block has nothing to end with a full stop.
        (
            "feat(core): add the thing\n\nCo-authored-by: A Person\nSigned-off-by: A Person\n",
            vec![],
        ),
        // Without a full stop above the block the body still fails.
        (
            "feat(core): add the thing\n\nThe body says why\n\nCo-authored-by: A Person\n",
            vec!["body-full-stop"],
        ),
        // A last paragraph with prose in it is no trailer block, so it is the body's end.
        (
            "feat(core): add the thing\n\nThe body says why.\n\nCo-authored-by: A Person\nand a line of prose\n",
            vec!["body-full-stop"],
        ),
        // Prose that the quarter rule keeps in the block is not judged as the body's end.
        (
            "feat(core): add the thing\n\nThe body says why.\n\nA note\nSigned-off-by: A Person\n",
            vec![],
        ),
        // A trailer-shaped paragraph above the last is body, and the last one ends it.
        (
            "feat(core): add the thing\n\nCo-authored-by: A Person\n\nThe body says why\n",
            vec!["body-full-stop"],
        ),
    ] {
        assert_eq!(aspects(Some(message), "{}"), expected, "{message:?}");
    }
}

#[test]
fn body_full_stop_reads_above_a_trailer_block_whose_reference_starts_the_footer() {
    // The parser ends the body at the issue reference, inside the trailer block.
    let full_stop = |message: &str| {
        aspects(Some(message), "{}")
            .into_iter()
            .filter(|aspect| aspect == "body-full-stop")
            .count()
    };
    let block = "\n\nCo-authored-by: A Person\nRefs: #12\n";
    assert_eq!(
        full_stop(&format!(
            "feat(core): add the thing\n\nDo the thing.{block}"
        )),
        0
    );
    assert_eq!(
        full_stop(&format!("feat(core): add the thing\n\nDo the thing{block}")),
        1
    );
    // The same with carriage returns.
    let crlf = format!("feat(core): add the thing\n\nDo the thing.{block}").replace('\n', "\r\n");
    assert_eq!(full_stop(&crlf), 0);
    let crlf = format!("feat(core): add the thing\n\nDo the thing{block}").replace('\n', "\r\n");
    assert_eq!(full_stop(&crlf), 1);
    // Text with more than one byte per character above the block.
    assert_eq!(
        full_stop(&format!(
            "feat(core): add the thing\n\nDo the thing \u{2014} twice.{block}"
        )),
        0
    );
}

#[test]
fn a_pushed_commit_s_prohibited_body_line_is_reported_on_that_commit() {
    let (mut repo, base, [_, _, third]) = branch([
        "feat(core): add the thing",
        "fix(core): mend the thing",
        "fix(core): mend it again\n\nThe body says why.\nGenerated with an assistant.",
    ]);
    repo.set_push(Some(&base), &third, "refs/heads/main");
    let found = pushed(&repo, NO_GENERATED);
    assert_eq!(found.len(), 1, "{found:?}");
    let (aspect, location, target, key, message) = &found[0];
    assert_eq!(aspect, "prohibited-body-lines");
    assert_eq!(
        *location,
        format!("{:?}", Some(gate_sdk::Location::commit(third.clone())))
    );
    assert_eq!(*target, third);
    assert_eq!(*key, format!("{third} Generated with an assistant."));
    assert!(
        message.starts_with(&format!("{}: the body line", &third[..12])),
        "{message}"
    );
}

/// A remote main at `base` and a branch of three commits on it, with `messages`, oldest first:
/// the repository, `base` and the three ids, oldest first.
fn branch(messages: [&str; 3]) -> (Repo, String, [String; 3]) {
    let mut repo = Repo::default();
    let base = repo.commit(&[], "chore: init", &[("README.md", Some("# r\n"))]);
    repo.set_ref("refs/heads/main", &base);
    let first = repo.commit(&[&base], messages[0], &[("a.txt", Some("a\n"))]);
    let second = repo.commit(&[&first], messages[1], &[("b.txt", Some("b\n"))]);
    let third = repo.commit(&[&second], messages[2], &[("c.txt", Some("c\n"))]);
    repo.set_head(&third);
    (repo, base, [first, second, third])
}

/// The `[plugins.git] options` of a push: main is the remote's main, else the local one.
const MAIN: &str = r#"{"main": ["refs/remotes/origin/main", "refs/heads/main"]}"#;

/// The findings of `repo` as aspect, location, target, key and message.
fn pushed(repo: &Repo, options: &str) -> Vec<(String, String, String, String, String)> {
    repo.check("commit-message", MAIN, options)
        .expect("runs")
        .into_iter()
        .map(|finding| {
            (
                finding.aspect,
                format!("{:?}", finding.location),
                finding.target.unwrap_or_default(),
                finding.key.unwrap_or_default(),
                finding.message,
            )
        })
        .collect()
}

const PUSH_GOOD: [&str; 3] = [
    "feat(core): add the thing",
    "fix(core): mend the thing",
    "docs(guide): say how the thing works",
];

#[test]
fn a_push_whose_commits_all_pass_passes() {
    let (mut repo, base, [.., head]) = branch(PUSH_GOOD);
    repo.set_push(Some(&base), &head, "refs/heads/main");
    assert_eq!(pushed(&repo, NO_AGENT), []);
}

#[test]
fn each_pushed_commit_that_fails_is_reported_on_that_commit() {
    let (mut repo, base, [first, second, third]) = branch([
        "feature(core): add the thing",
        "fix(core): mend the thing",
        "fix(core): mend it again\n\nThe body says why.\n\nCo-authored-by: Example Agent",
    ]);
    repo.set_push(Some(&base), &third, "refs/heads/main");
    let found = pushed(&repo, NO_AGENT);
    let short = |id: &str| id[..12].to_owned();
    assert_eq!(
        found
            .iter()
            .map(|(aspect, location, target, key, _)| (
                aspect.as_str(),
                location.clone(),
                target.clone(),
                key.clone()
            ))
            .collect::<Vec<_>>(),
        [
            (
                "type-enum",
                format!("{:?}", Some(gate_sdk::Location::commit(first.clone()))),
                first.clone(),
                first.clone(),
            ),
            // The body ends with a full stop above the trailer block, so it passes.
            (
                "prohibited-trailers",
                format!("{:?}", Some(gate_sdk::Location::commit(third.clone()))),
                third.clone(),
                format!("{third} Co-authored-by: Example Agent"),
            ),
        ]
    );
    assert!(
        found[0]
            .4
            .starts_with(&format!("{}: type feature", short(&first))),
        "{found:?}"
    );
    assert!(
        found[1]
            .4
            .starts_with(&format!("{}: the trailer", short(&third))),
        "{found:?}"
    );
    assert!(
        !found
            .iter()
            .any(|(.., message)| message.contains(&short(&second)))
    );
}

#[test]
fn a_merge_and_a_fixup_in_a_push_are_skipped_by_the_default_ignores() {
    let (mut repo, base, [first, second, third]) = branch([
        "feat(core): add the thing",
        "fixup! feat(core): add the thing",
        "Merge branch 'main' into feat/the-thing",
    ]);
    repo.set_push(Some(&base), &third, "refs/heads/main");
    assert_eq!(pushed(&repo, "{}"), []);
    // Without the default ignores both are checked, oldest first.
    let keys: Vec<String> = pushed(&repo, r#"{"default-ignores": false}"#)
        .into_iter()
        .map(|(.., key, _)| key)
        .collect();
    assert!(!keys.contains(&first), "{keys:?}");
    let at = |id: &String| keys.iter().position(|key| key == id);
    assert!(
        at(&second).zip(at(&third)).is_some_and(|(s, t)| s < t),
        "{keys:?}"
    );
}

/// The keys of `repo`'s findings, in their order.
fn pushed_keys(repo: &Repo) -> Vec<String> {
    pushed(repo, "{}")
        .into_iter()
        .map(|(.., key, _)| key)
        .collect()
}

#[test]
fn a_new_branch_judges_every_commit_it_adds_to_main_and_none_of_main() {
    let (mut repo, base, [first, second, third]) = branch([
        "feature(core): add the thing",
        "feature(core): mend the thing",
        "feature(core): mend it again",
    ]);
    // Main moves on with a commit of another form, which the branch then merges.
    let web = repo.commit(
        &[&base],
        "Update README.md",
        &[("README.md", Some("# s\n"))],
    );
    repo.set_ref("refs/remotes/origin/main", &web);
    let merge = repo.commit(
        &[&third, &web],
        "Merge branch 'main' into feat/the-thing",
        &[],
    );
    repo.set_push(None, &merge, "refs/heads/feat/the-thing");
    assert_eq!(pushed_keys(&repo), [first, second, third]);
}

#[test]
fn a_merge_of_main_into_a_pushed_branch_judges_only_the_branch_s_new_commits() {
    let (mut repo, base, [first, ..]) = branch([
        "feature(core): add the thing",
        "fix(core): mend the thing",
        "fix(core): mend it again",
    ]);
    // The remote branch is at `first`; main gets a web edit; the branch merges main and adds one.
    let web = repo.commit(
        &[&base],
        "Update README.md",
        &[("README.md", Some("# s\n"))],
    );
    repo.set_ref("refs/remotes/origin/main", &web);
    let merge = repo.commit(
        &[&first, &web],
        "Merge branch 'main' into feat/the-thing",
        &[],
    );
    let next = repo.commit(
        &[&merge],
        "feature(core): the next thing",
        &[("d.txt", Some("d\n"))],
    );
    repo.set_push(Some(&first), &next, "refs/heads/feat/the-thing");
    assert_eq!(pushed_keys(&repo), [next]);
}

#[test]
fn a_rebase_and_force_push_judges_only_the_branch_s_rewritten_commits() {
    let (mut repo, base, [first, ..]) = branch([
        "feature(core): add the thing",
        "fix(core): mend the thing",
        "fix(core): mend it again",
    ]);
    // The remote branch is at `first`; main gets a web edit; the branch is rebased onto it.
    let web = repo.commit(
        &[&base],
        "Update README.md",
        &[("README.md", Some("# s\n"))],
    );
    repo.set_ref("refs/remotes/origin/main", &web);
    let rebased = repo.commit(
        &[&web],
        "feature(core): add the thing",
        &[("a.txt", Some("a\n"))],
    );
    repo.set_push(Some(&first), &rebased, "refs/heads/feat/the-thing");
    assert_eq!(pushed_keys(&repo), [rebased]);
}

#[test]
fn a_pushed_tag_checks_nothing() {
    let (mut repo, base, [.., third]) = branch([
        "feature(core): add the thing",
        "feature(core): mend the thing",
        "feature(core): mend it again",
    ]);
    repo.set_push(None, &third, "refs/tags/v1.0.0");
    assert_eq!(pushed_keys(&repo), Vec::<String>::new());
    repo.set_push(Some(&base), &third, "refs/tags/v1.0.0");
    assert_eq!(pushed_keys(&repo), Vec::<String>::new());
}

#[test]
fn a_pushed_commit_s_help_says_how_to_reword_it_or_except_it() {
    let (mut repo, base, [first, ..]) = branch([
        "feature(core): add the thing",
        "fix(core): mend the thing",
        "fix(core): mend it again",
    ]);
    repo.set_push(Some(&base), &first, "refs/heads/feat/the-thing");
    let found = repo.check("commit-message", MAIN, "{}").expect("runs");
    let help = found
        .first()
        .and_then(|finding| finding.help.clone())
        .unwrap_or_default();
    assert!(
        help.contains(&format!("git rebase -i {}^", &first[..12]))
            && help.contains("an exception for its id"),
        "{help}"
    );
    // The message being written keeps the help of the message alone.
    repo.set_range(&first, None);
    repo.set_message(Some("feature(core): the next thing\n"));
    let found = repo.check("commit-message", MAIN, "{}").expect("runs");
    let help = found
        .first()
        .and_then(|finding| finding.help.clone())
        .unwrap_or_default();
    assert!(!help.contains("git rebase"), "{help}");
}

#[test]
fn a_rev_run_and_a_run_with_neither_range_nor_message_check_nothing() {
    let (mut repo, _, [.., head]) = branch([
        "feature(core): add the thing",
        "feature(core): mend the thing",
        "feature(core): mend it again",
    ]);
    assert_eq!(pushed(&repo, "{}"), []);
    repo.set_range(&head, None);
    assert_eq!(pushed(&repo, "{}"), []);
}

#[test]
fn the_message_being_written_and_a_push_are_checked_in_one_run() {
    let (mut repo, base, [first, _, third]) = branch([
        "feature(core): add the thing",
        "fix(core): mend the thing",
        "fix(core): mend it again",
    ]);
    repo.set_push(Some(&base), &third, "refs/heads/main");
    repo.set_message(Some("feature(core): the next thing\n"));
    let found: Vec<(String, String, String)> = pushed(&repo, "{}")
        .into_iter()
        .map(|(aspect, location, target, ..)| (aspect, location, target))
        .collect();
    assert_eq!(
        found,
        [
            (
                "type-enum".to_owned(),
                "None".to_owned(),
                "feature(core): the next thing".to_owned()
            ),
            (
                "type-enum".to_owned(),
                format!("{:?}", Some(gate_sdk::Location::commit(first.clone()))),
                first,
            ),
        ]
    );
}

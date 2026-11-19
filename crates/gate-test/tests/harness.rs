//! The harness on cases written for the test: a difference fails and names the case, bless writes
//! what the rule reports, and a case or a rule that cannot run fails.

use std::path::Path;

use gate_sdk::{Location, Rule, Run, schemars};
use gate_test::{Mode, check_fixtures_in};

/// Reports every file whose text holds `word`; fails when `word` is empty.
#[derive(Debug, Default)]
pub struct Word;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WordOptions {
    #[serde(default)]
    word: String,
}

impl Rule for Word {
    type Options = WordOptions;
    const NAME: &'static str = "word";
    const TITLE: &'static str = "Reports a word";

    fn check(&self, run: &mut Run<'_, WordOptions>) -> gate_sdk::Result<()> {
        let word = run.options().word.clone();
        if word.is_empty() {
            return Err(gate_sdk::Error::new("word: set the option word"));
        }
        let tree = run.tree();
        for path in tree.under("src") {
            if tree.text(&path).is_some_and(|text| text.contains(&word)) {
                run.report()
                    .finding("", Location::line(path.clone(), 1), format!("holds {word}"))
                    .target(path);
            }
        }
        Ok(())
    }
}

gate_sdk::plugin! {
    name: "words",
    version: "0.0.0",
    options: gate_sdk::NoOptions,
    rules: [Word],
    generators: [],
    presets: [],
}

fn write(root: &Path, path: &str, contents: &str) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().expect("parent")).expect("directories");
    std::fs::write(full, contents).expect("write");
}

/// One case `hit` whose tree has `src/a.txt` holding the word, `src/b.txt` without it, and
/// `srcs/c.txt` outside `src/`.
fn fixtures() -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path();
    write(root, "hit/tree/src/a.txt", "one word here\n");
    write(root, "hit/tree/src/b.txt", "nothing\n");
    write(root, "hit/tree/srcs/c.txt", "word, outside src/\n");
    write(root, "hit/options.json", "{ \"word\": \"word\" }\n");
    directory
}

const HIT: &str = "src/a.txt:1:1 word: holds word\n  target: src/a.txt\n";

#[test]
fn bless_writes_what_the_rule_reports_and_compare_then_passes() {
    let directory = fixtures();
    check_fixtures_in(&PLUGIN, "word", directory.path(), Mode::Bless).expect("bless");
    let written =
        std::fs::read_to_string(directory.path().join("hit/expected.txt")).expect("written");
    assert_eq!(written, HIT);
    check_fixtures_in(&PLUGIN, "word", directory.path(), Mode::Compare).expect("compare");
}

#[test]
fn a_difference_fails_naming_the_case_and_both_sides() {
    let directory = fixtures();
    write(directory.path(), "hit/expected.txt", "");
    let failure = check_fixtures_in(&PLUGIN, "word", directory.path(), Mode::Compare)
        .expect_err("a finding the expected file lacks fails");
    let message = failure.to_string();
    assert!(
        message.contains("case hit: the findings differ from expected.txt"),
        "{message}"
    );
    assert!(message.contains(HIT), "{message}");
    assert!(message.contains("GATE_BLESS=1"), "{message}");

    write(directory.path(), "hit/expected.txt", &format!("{HIT}{HIT}"));
    assert!(
        check_fixtures_in(&PLUGIN, "word", directory.path(), Mode::Compare).is_err(),
        "a finding the rule no longer reports fails"
    );
}

#[test]
fn a_case_without_expected_findings_fails_and_shows_what_the_rule_reports() {
    let directory = fixtures();
    let message = check_fixtures_in(&PLUGIN, "word", directory.path(), Mode::Compare)
        .expect_err("no expected.txt fails")
        .to_string();
    assert!(message.contains("case hit: no expected.txt"), "{message}");
    assert!(message.contains(HIT), "{message}");
}

#[test]
fn a_rule_error_an_unknown_rule_and_an_empty_directory_fail() {
    let directory = fixtures();
    write(directory.path(), "hit/options.json", "{}\n");
    write(directory.path(), "hit/expected.txt", "");
    let message = check_fixtures_in(&PLUGIN, "word", directory.path(), Mode::Compare)
        .expect_err("the rule's error fails")
        .to_string();
    assert!(
        message.contains("case hit: rule word failed: word: set the option word"),
        "{message}"
    );

    let message = check_fixtures_in(&PLUGIN, "other", directory.path(), Mode::Compare)
        .expect_err("an unknown rule fails")
        .to_string();
    assert!(message.contains("has no rule \"other\""), "{message}");

    let empty = tempfile::tempdir().expect("temporary directory");
    let message = check_fixtures_in(&PLUGIN, "word", empty.path(), Mode::Compare)
        .expect_err("no case fails")
        .to_string();
    assert!(message.contains("has no case"), "{message}");
}

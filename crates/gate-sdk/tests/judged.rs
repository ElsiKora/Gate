//! `Run::judges_only`: a rule that never calls it judged its whole scope and is not listed in
//! what `check` answers; one that calls it is listed with the targets it gave, without repeats.

use gate_sdk::{
    CheckRequest, GitHost, Host, Judged, Location, NoOptions, Result, Rule, RuleRun, Run, TreeHost,
};

/// Judges its whole scope: never narrows.
#[derive(Default)]
struct Whole;

impl Rule for Whole {
    type Options = NoOptions;
    const NAME: &'static str = "whole";
    const TITLE: &'static str = "Judges its whole scope";

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        run.report()
            .finding("", Location::line("a.txt", 1), "whole");
        Ok(())
    }
}

/// Judges the subject it checks and the excepted targets that start with `keep`.
#[derive(Default)]
struct Narrow;

impl Rule for Narrow {
    type Options = NoOptions;
    const NAME: &'static str = "narrow";
    const TITLE: &'static str = "Judges what it names";

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let kept: Vec<String> = run
            .exceptions()
            .iter()
            .filter(|target| target.starts_with("keep"))
            .cloned()
            .collect();
        run.judges_only(["subject"]);
        run.judges_only(kept);
        run.judges_only(["subject"]);
        Ok(())
    }
}

/// Judges nothing in this run.
#[derive(Default)]
struct Nothing;

impl Rule for Nothing {
    type Options = NoOptions;
    const NAME: &'static str = "nothing";
    const TITLE: &'static str = "Judges nothing";

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        run.judges_only(Vec::<String>::new());
        Ok(())
    }
}

gate_sdk::plugin! {
    name: "judges",
    version: "0.0.0",
    options: NoOptions,
    rules: [Whole, Narrow, Nothing],
    generators: [],
    presets: [],
}

struct Empty;

impl TreeHost for Empty {
    fn list(&self, _prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn read(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Empty {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

fn rule(name: &str, exceptions: &[&str]) -> RuleRun {
    RuleRun {
        name: name.to_owned(),
        options: "{}".to_owned(),
        options_file: None,
        exceptions: exceptions
            .iter()
            .map(|target| (*target).to_owned())
            .collect(),
    }
}

#[test]
fn only_the_rules_that_narrow_are_listed_with_what_they_judged() {
    let request = CheckRequest {
        rules: vec![
            rule("whole", &["keep-a"]),
            rule("narrow", &["keep-a", "drop", "keep-b", "subject"]),
            rule("nothing", &["keep-a"]),
        ],
        ..CheckRequest::default()
    };
    let output = gate_sdk::check_output(&PLUGIN, &Empty, &request).expect("the call runs");
    assert_eq!(
        output.judged,
        [
            Judged {
                rule: "narrow".to_owned(),
                targets: vec![
                    "subject".to_owned(),
                    "keep-a".to_owned(),
                    "keep-b".to_owned()
                ],
            },
            Judged {
                rule: "nothing".to_owned(),
                targets: Vec::new(),
            },
        ]
    );
    let findings = gate_sdk::check(&PLUGIN, &Empty, &request).expect("the call runs");
    assert_eq!(findings, output.findings);
    assert_eq!(findings.len(), 1);
}

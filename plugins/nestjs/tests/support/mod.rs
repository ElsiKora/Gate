//! The fixture host the rule tests run the plugin against: a synthetic tree of tracked files, no
//! git, and one rule per call.

#![allow(
    dead_code,
    reason = "each test crate uses its own subset of the helpers"
)]

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// A fixture tree: tracked paths and their contents.
pub struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
}

impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
                .collect(),
        }
    }
}

impl TreeHost for Fixture {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Fixture {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// Runs the rule `name` with `options` (JSON) and `plugin_options` (the `[plugins.nestjs]
/// options` JSON) over `files`.
pub fn run(
    name: &str,
    options: &str,
    plugin_options: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    run_from(name, options, None, plugin_options, files)
}

/// Runs the rule `name` with `options` (JSON) read from `options_file`, a file of `files`.
pub fn run_from(
    name: &str,
    options: &str,
    options_file: Option<&str>,
    plugin_options: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    run_in(name, options, options_file, plugin_options, "{}", files)
}

/// Runs the rule `name` with `languages` (the `[languages]` JSON) as the language settings.
pub fn run_with_languages(
    name: &str,
    languages: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    run_in(name, "{}", None, "{}", languages, files)
}

/// Runs the rule `name` with `options` (JSON), `plugin_options` (the `[plugins.nestjs] options`
/// JSON) and `languages` (the `[languages]` JSON) over `files`.
pub fn run_with(
    name: &str,
    options: &str,
    plugin_options: &str,
    languages: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    run_in(name, options, None, plugin_options, languages, files)
}

/// The language settings that add `generated` as the only skipped folder of TypeScript.
pub const SKIP_GENERATED: &str = r#"{ "typescript": { "skip-folders": ["generated"] } }"#;

/// Runs the rule like [`run_with`] and returns the paths its findings name, in report order, with
/// consecutive repeats of one path kept once.
pub fn paths_with(
    name: &str,
    options: &str,
    plugin_options: &str,
    languages: &str,
    files: &[(&str, &str)],
) -> Vec<String> {
    let mut paths: Vec<String> =
        rows(&run_with(name, options, plugin_options, languages, files).expect("the rule runs"))
            .into_iter()
            .map(|(_, path, _, _)| path)
            .collect();
    paths.dedup();
    paths
}

fn run_in(
    name: &str,
    options: &str,
    options_file: Option<&str>,
    plugin_options: &str,
    languages: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    gate_sdk::check(
        &nestjs::PLUGIN,
        &Fixture::new(files),
        &CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: options_file.map(str::to_owned),
                exceptions: Vec::new(),
            }],
            plugin_options: plugin_options.to_owned(),
            languages: languages.to_owned(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
            config_file: String::new(),
        },
    )
}

/// One finding as the tests compare it: aspect, path, line and message.
pub type Row = (String, String, Option<u32>, String);

/// The findings as rows, in report order.
pub fn rows(findings: &[Finding]) -> Vec<Row> {
    findings
        .iter()
        .map(|finding| {
            let (path, line) = match &finding.location {
                Some(Location::File(file)) => (file.path.clone(), file.span.map(|span| span.line)),
                Some(Location::Commit(id)) => (format!("commit {id}"), None),
                None => (String::new(), None),
            };
            (finding.aspect.clone(), path, line, finding.message.clone())
        })
        .collect()
}

/// Runs the rule with the default layout and returns its findings as rows.
pub fn check(name: &str, options: &str, files: &[(&str, &str)]) -> Vec<Row> {
    rows(&run(name, options, "{}", files).expect("the rule runs"))
}

/// Runs the rule with the default layout and returns its error message.
pub fn error(name: &str, options: &str, files: &[(&str, &str)]) -> String {
    run(name, options, "{}", files)
        .expect_err("the rule fails")
        .message()
        .to_owned()
}

/// A row.
pub fn row(aspect: &str, path: &str, line: Option<u32>, message: &str) -> Row {
    (aspect.to_owned(), path.to_owned(), line, message.to_owned())
}

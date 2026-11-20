//! The fixture host the semantic rule tests run the plugin against: a synthetic tree of tracked
//! files, no git, and one rule per call.

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

    /// The tree with `path` holding `bytes`, which need not be UTF-8.
    #[must_use]
    pub fn with(mut self, path: &str, bytes: &[u8]) -> Self {
        self.files.insert(path.to_owned(), bytes.to_vec());
        self
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

/// One rule run: its options, options file, plugin options, exception targets and norms.
#[derive(Default)]
pub struct Request<'a> {
    pub options: &'a str,
    pub options_file: Option<&'a str>,
    pub plugin_options: &'a str,
    /// The `[languages]` JSON; empty means none.
    pub languages: &'a str,
    pub exceptions: Vec<String>,
    pub norms: Vec<String>,
}

/// Runs the rule `name` as `request` describes over `files`.
pub fn run_with(
    name: &str,
    request: &Request<'_>,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    run_on(name, request, &Fixture::new(files))
}

/// Runs the rule `name` as `request` describes over `fixture`.
pub fn run_on(
    name: &str,
    request: &Request<'_>,
    fixture: &Fixture,
) -> gate_sdk::Result<Vec<Finding>> {
    let or_empty = |json: &str| {
        if json.is_empty() {
            "{}".to_owned()
        } else {
            json.to_owned()
        }
    };

    gate_sdk::check(
        &nestjs::PLUGIN,
        fixture,
        &CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: or_empty(request.options),
                options_file: request.options_file.map(str::to_owned),
                exceptions: request.exceptions.clone(),
            }],
            plugin_options: or_empty(request.plugin_options),
            languages: or_empty(request.languages),
            norms: request.norms.clone(),
            rule_decisions: Vec::new(),
        },
    )
}

/// Runs the rule `name` with `options` (JSON) and `plugin_options` (the `[plugins.nestjs]
/// options` JSON) over `files`.
pub fn run(
    name: &str,
    options: &str,
    plugin_options: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    run_with(
        name,
        &Request {
            options,
            plugin_options,
            ..Request::default()
        },
        files,
    )
}

/// One finding as the tests compare it: aspect, path, line and message.
pub type Row = (String, String, Option<u32>, String);

/// The location of a finding as a path and a line.
fn place(finding: &Finding) -> (String, Option<u32>) {
    match &finding.location {
        Some(Location::File(file)) => (file.path.clone(), file.span.map(|span| span.line)),
        Some(Location::Commit(id)) => (format!("commit {id}"), None),
        None => (String::new(), None),
    }
}

/// The findings as rows, in report order.
pub fn rows(findings: &[Finding]) -> Vec<Row> {
    findings
        .iter()
        .map(|finding| {
            let (path, line) = place(finding);
            (finding.aspect.clone(), path, line, finding.message.clone())
        })
        .collect()
}

/// The findings' targets, in report order.
pub fn targets(findings: &[Finding]) -> Vec<Option<String>> {
    findings
        .iter()
        .map(|finding| finding.target.clone())
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

/// `[languages]` with `skip-folders` set to `["generated"]` only: a custom list replaces the
/// default, so `node_modules` is read again.
pub const SKIP_GENERATED: &str = r#"{ "typescript": { "skip-folders": ["generated"] } }"#;

/// `[languages]` whose `skip-folders` name `dist` at any depth and `generated`.
pub const SKIP_DIST: &str = r#"{ "typescript": { "skip-folders": ["dist", "generated"] } }"#;

/// Runs the rule with `options` and the `[languages]` JSON `languages` over `files` and returns the
/// paths its findings name, sorted, without repeats.
pub fn paths_with(
    name: &str,
    options: &str,
    languages: &str,
    files: &[(&str, &str)],
) -> Vec<String> {
    let findings = run_with(
        name,
        &Request {
            options,
            languages,
            ..Request::default()
        },
        files,
    )
    .expect("the rule runs");
    let paths: std::collections::BTreeSet<String> =
        rows(&findings).into_iter().map(|row| row.1).collect();

    paths.into_iter().collect()
}

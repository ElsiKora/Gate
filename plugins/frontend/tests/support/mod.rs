//! The fixture host the rule tests run the plugin against: a synthetic tree of tracked files, no
//! git, and one rule per call.

#![allow(
    dead_code,
    reason = "each test module uses its own subset of the helpers"
)]

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// A fixture tree: tracked paths and their contents.
struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
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

/// The tsconfig every fixture tracks: the one alias, `@/` for `src/`.
pub const TSCONFIG: (&str, &str) = (
    "tsconfig.json",
    r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["./src/*"] } } }"#,
);

/// Runs the rule `name` with `options` (JSON) and `plugin_options` (JSON) over `files`, and
/// [`TSCONFIG`] when `files` track no tsconfig of their own.
pub fn run_raw(
    name: &str,
    options: &str,
    plugin_options: &str,
    files: &[(&str, &str)],
) -> gate_sdk::Result<Vec<Finding>> {
    let fixture = Fixture {
        files: files
            .iter()
            .chain((!files.iter().any(|(path, _)| *path == TSCONFIG.0)).then_some(&TSCONFIG))
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
            .collect(),
    };
    gate_sdk::check(
        &frontend::PLUGIN,
        &fixture,
        &CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: plugin_options.to_owned(),
            languages: "{}".to_owned(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
            config_file: String::new(),
        },
    )
}

/// One finding as the tests compare it: aspect, path and line.
pub type Row = (String, String, Option<u32>);

/// Runs the rule with the pack `pack` and `options` and returns its findings as rows, sorted.
pub fn check_with(name: &str, pack: &str, options: &str, files: &[(&str, &str)]) -> Vec<Row> {
    let findings = run_raw(name, options, &format!(r#"{{ "pack": "{pack}" }}"#), files)
        .expect("the rule runs");
    let mut rows: Vec<Row> = findings
        .iter()
        .map(|finding| {
            let (path, line) = match &finding.location {
                Some(Location::File(file)) => (file.path.clone(), file.span.map(|span| span.line)),
                _ => (String::new(), None),
            };
            (finding.aspect.clone(), path, line)
        })
        .collect();
    rows.sort();
    rows
}

/// Runs the rule with the pack `pack` and default options.
pub fn check(name: &str, pack: &str, files: &[(&str, &str)]) -> Vec<Row> {
    check_with(name, pack, "{}", files)
}

/// A row.
pub fn row(aspect: &str, path: &str, line: Option<u32>) -> Row {
    (aspect.to_owned(), path.to_owned(), line)
}

/// The messages of the rule's findings, for the tests that read one.
pub fn messages(name: &str, pack: &str, files: &[(&str, &str)]) -> Vec<String> {
    run_raw(name, "{}", &format!(r#"{{ "pack": "{pack}" }}"#), files)
        .expect("the rule runs")
        .into_iter()
        .map(|finding| finding.message)
        .collect()
}

/// The help texts of the rule's findings, for the tests that read one.
pub fn helps(name: &str, pack: &str, files: &[(&str, &str)]) -> Vec<String> {
    run_raw(name, "{}", &format!(r#"{{ "pack": "{pack}" }}"#), files)
        .expect("the rule runs")
        .into_iter()
        .filter_map(|finding| finding.help)
        .collect()
}

/// `files` without the file `path`, to show that the piece is required.
pub fn without<'f>(files: &[(&'f str, &'f str)], path: &str) -> Vec<(&'f str, &'f str)> {
    assert!(
        files.iter().any(|(tracked, _)| *tracked == path),
        "{path} is in the tree"
    );
    files
        .iter()
        .copied()
        .filter(|(tracked, _)| *tracked != path)
        .collect()
}

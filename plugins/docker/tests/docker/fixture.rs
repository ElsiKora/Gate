//! A fixture tree for the native runs: tracked paths and their bytes, no git.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, RuleRun, TreeHost};

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

    /// Runs rule `name` of the docker plugin with `options` and `plugin_options` (JSON).
    pub fn check_with(
        &self,
        name: &str,
        options: &str,
        plugin_options: &str,
    ) -> gate_sdk::Result<Vec<Finding>> {
        let request = CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: plugin_options.to_owned(),
            languages: String::new(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
            config_file: String::new(),
        };
        gate_sdk::check(&docker::PLUGIN, self, &request)
    }

    /// The findings of rule `name` with its default options, as `aspect path:line key`.
    pub fn rows(&self, name: &str) -> Vec<String> {
        rows(&self.check_with(name, "{}", "").expect("the rule runs"))
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

/// Each finding as `aspect path:line key`, the line 0 for none, sorted.
pub fn rows(findings: &[Finding]) -> Vec<String> {
    let mut rows: Vec<String> = findings
        .iter()
        .map(|finding| {
            let Some(gate_sdk::Location::File(location)) = &finding.location else {
                panic!("a finding names its file: {finding:?}");
            };
            format!(
                "{} {}:{} {}",
                finding.aspect,
                location.path,
                location.span.map_or(0, |span| span.line),
                finding.key.as_deref().unwrap_or("")
            )
        })
        .collect();
    rows.sort();
    rows
}

/// Every text of every finding, for checks that a value never appears.
pub fn texts(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{} {} {} {}",
                finding.message,
                finding.help.as_deref().unwrap_or(""),
                finding.target.as_deref().unwrap_or(""),
                finding.key.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

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

    pub fn with_bytes(mut self, path: &str, bytes: &[u8]) -> Self {
        self.files.insert(path.to_owned(), bytes.to_vec());
        self
    }

    /// Runs rule `name` of the markdown plugin with `options` (JSON).
    pub fn check(&self, name: &str, options: &str) -> gate_sdk::Result<Vec<Finding>> {
        let request = CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: String::new(),
            languages: String::new(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
        };
        gate_sdk::check(&markdown::PLUGIN, self, &request)
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

/// Each finding as `path:line message`, or `path message` without a line.
pub fn lines(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| {
            let Some(gate_sdk::Location::File(location)) = &finding.location else {
                panic!("a finding names its file: {finding:?}");
            };
            match location.span {
                Some(span) => format!("{}:{} {}", location.path, span.line, finding.message),
                None => format!("{} {}", location.path, finding.message),
            }
        })
        .collect()
}

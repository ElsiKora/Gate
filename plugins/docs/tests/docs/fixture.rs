//! A fixture tree for the native runs: tracked paths and their bytes, no git.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GenerateOutput, GitHost, Host, RuleRun, TreeHost};

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

    /// The tree a `gate sync` leaves: every file `files` holds.
    pub fn from_output(output: &GenerateOutput) -> Self {
        Self {
            files: output
                .files
                .iter()
                .map(|file| (file.path.clone(), file.contents.clone()))
                .collect(),
        }
    }

    pub fn with(mut self, path: &str, text: &str) -> Self {
        self.files.insert(path.to_owned(), text.as_bytes().to_vec());
        self
    }

    pub fn without(mut self, path: &str) -> Self {
        self.files.remove(path);
        self
    }

    /// Every file's text with `from` replaced by `to`.
    pub fn replaced(mut self, from: &str, to: &str) -> Self {
        for contents in self.files.values_mut() {
            let text = String::from_utf8_lossy(contents).replace(from, to);
            *contents = text.into_bytes();
        }
        self
    }

    /// Runs `docs/layout` with the plugin options `plugin_options` (JSON).
    pub fn check(&self, plugin_options: &str) -> gate_sdk::Result<Vec<Finding>> {
        let request = CheckRequest {
            project: String::new(),
            rules: vec![RuleRun {
                name: "layout".to_owned(),
                options: String::new(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: plugin_options.to_owned(),
            languages: String::new(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
        };
        gate_sdk::check(&docs::PLUGIN, self, &request)
    }

    /// Runs the generator `docs/layout` with the plugin options `plugin_options` (JSON).
    pub fn generate(&self, plugin_options: &str) -> gate_sdk::Result<GenerateOutput> {
        gate_sdk::generate(&docs::PLUGIN, self, "layout", "", "{}", plugin_options)
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

/// The plugin options of `kind`.
pub fn kind(kind: &str) -> String {
    format!(r#"{{"kind": "{kind}"}}"#)
}

/// Each finding as `aspect path[:line] key`.
pub fn keys(findings: &[Finding]) -> Vec<String> {
    let mut keys: Vec<String> = findings
        .iter()
        .map(|finding| {
            let Some(gate_sdk::Location::File(location)) = &finding.location else {
                panic!("a finding names its file: {finding:?}");
            };
            let at = match location.span {
                Some(span) => format!("{}:{}", location.path, span.line),
                None => location.path.clone(),
            };
            format!(
                "{} {at} {}",
                finding.aspect,
                finding.key.as_deref().unwrap_or("-")
            )
        })
        .collect();
    keys.sort();
    keys
}

/// The finding messages.
pub fn messages(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| finding.message.clone())
        .collect()
}

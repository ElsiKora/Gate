//! A fixture tree for the native runs: tracked paths and their bytes, no git.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

pub struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
    project: String,
    plugin_options: String,
}

impl Fixture {
    /// A tree of `files` checked as the repository's root project of kind `kind`.
    pub fn new(kind: &str, files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
                .collect(),
            project: String::new(),
            plugin_options: format!(r#"{{"kind": "{kind}"}}"#),
        }
    }

    /// The same tree with these `[plugins.node] options` (JSON).
    pub fn plugin_options(mut self, options: &str) -> Self {
        options.clone_into(&mut self.plugin_options);
        self
    }

    /// The same tree, seen as the project at `project` of the repository.
    pub fn project(mut self, project: &str) -> Self {
        project.clone_into(&mut self.project);
        self
    }

    /// Runs rule `name` of the node plugin with `options` (JSON).
    pub fn check(&self, name: &str, options: &str) -> gate_sdk::Result<Vec<Finding>> {
        let request = CheckRequest {
            project: self.project.clone(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: self.plugin_options.clone(),
            languages: String::new(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
        };
        gate_sdk::check(&node::PLUGIN, self, &request)
    }

    /// The findings of rule `name` with its default options, as [`lines`].
    pub fn lines(&self, name: &str) -> Vec<String> {
        lines(&self.check(name, "{}").expect("the rule runs"))
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

/// Each finding as `aspect path[:line] key`.
pub fn lines(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| {
            let Some(Location::File(location)) = &finding.location else {
                panic!("a finding names its file: {finding:?}");
            };
            let place = match location.span {
                Some(span) => format!("{}:{}", location.path, span.line),
                None => location.path.clone(),
            };
            format!(
                "{} {place} {}",
                finding.aspect,
                finding.key.as_deref().unwrap_or_default()
            )
        })
        .collect()
}

/// The message of the only finding of rule `name`.
pub fn message(fixture: &Fixture, name: &str) -> String {
    let findings = fixture.check(name, "{}").expect("the rule runs");
    assert_eq!(findings.len(), 1, "{:?}", lines(&findings));
    findings[0].message.clone()
}

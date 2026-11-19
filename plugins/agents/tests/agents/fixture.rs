//! A fixture tree for the native runs: tracked paths and their bytes, no git.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use gate_sdk::{CheckRequest, Finding, GenerateOutput, GitHost, Host, Location, RuleRun, TreeHost};

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

    /// The same tree with `path` holding `bytes`, which need not be UTF-8.
    pub fn with_bytes(mut self, path: &str, bytes: &[u8]) -> Self {
        self.files.insert(path.to_owned(), bytes.to_vec());
        self
    }

    /// Runs rule `name` with `options` (JSON) and no configured norms.
    pub fn check(&self, name: &str, options: &str) -> gate_sdk::Result<Vec<Finding>> {
        self.check_with_norms(name, options, &[])
    }

    /// Runs rule `name` with `options` (JSON) while the configuration maps `norms`.
    pub fn check_with_norms(
        &self,
        name: &str,
        options: &str,
        norms: &[&str],
    ) -> gate_sdk::Result<Vec<Finding>> {
        self.check_in("", name, options, norms)
    }

    /// Runs rule `name` in the project at `project`, whose tree this fixture is: the project's
    /// files relative to its root, granted paths outside it with `../`.
    pub fn check_in(
        &self,
        project: &str,
        name: &str,
        options: &str,
        norms: &[&str],
    ) -> gate_sdk::Result<Vec<Finding>> {
        let request = CheckRequest {
            project: project.to_owned(),
            rules: vec![RuleRun {
                name: name.to_owned(),
                options: options.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: String::new(),
            languages: String::new(),
            norms: norms.iter().map(|norm| (*norm).to_owned()).collect(),
            rule_decisions: Vec::new(),
        };
        gate_sdk::check(&agents::PLUGIN, self, &request)
    }

    /// Runs generator `name` for `project` with `options` (JSON).
    pub fn generate(
        &self,
        name: &str,
        project: &str,
        options: &str,
    ) -> gate_sdk::Result<GenerateOutput> {
        gate_sdk::generate(&agents::PLUGIN, self, name, project, options, "")
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

/// Each finding as `[aspect ]path[:line] message`, or `- message` without a location.
pub fn lines(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|finding| {
            let aspect = if finding.aspect.is_empty() {
                String::new()
            } else {
                format!("#{} ", finding.aspect)
            };
            match &finding.location {
                Some(Location::File(location)) => match location.span {
                    Some(span) => format!(
                        "{aspect}{}:{} {}",
                        location.path, span.line, finding.message
                    ),
                    None => format!("{aspect}{} {}", location.path, finding.message),
                },
                Some(Location::Commit(id)) => panic!("no agents rule reports a commit: {id}"),
                None => format!("{aspect}- {}", finding.message),
            }
        })
        .collect()
}

/// A rule file whose front matter is `front` and whose body is `body`.
pub fn rule_file(front: &str, body: &str) -> String {
    format!("---\n{front}---\n# Rules\n\n{body}")
}

/// A well-formed always-applying rule file of `area` with rules `1..=count`, all `gate`.
pub fn always(area: &str, count: u32) -> String {
    let mut body = String::new();
    for number in 1..=count {
        let _ = write!(
            body,
            "## RULE-{area}-{number:03} Rule {number}\n\nEnforcement: gate · Status: stable\n\n"
        );
    }
    rule_file(
        &format!("area: {area}\napplies: always\nowner: D-ONE-001\n"),
        &body,
    )
}

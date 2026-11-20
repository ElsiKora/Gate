//! The repository plugin's rules run natively through the SDK against synthetic trees.

mod large_files;
mod tracked_secrets;

use std::cell::Cell;
use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, RuleRun, Scope, TreeHost};
use serde_json::Value;

/// A fixture tree that counts the contents it hands out.
pub(crate) struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
    reads: Cell<usize>,
}

impl Fixture {
    pub(crate) fn new(files: &[(&str, &[u8])]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
                .collect(),
            reads: Cell::new(0),
        }
    }

    pub(crate) fn check(&self, name: &str, options: &str) -> gate_sdk::Result<Vec<Finding>> {
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
        gate_sdk::check(&repository::PLUGIN, self, &request)
    }

    /// How many times a file's contents were read.
    pub(crate) fn reads(&self) -> usize {
        self.reads.get()
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
        self.reads.set(self.reads.get() + 1);
        self.files.get(path).cloned()
    }

    fn size(&self, path: &str) -> Option<u64> {
        self.files
            .get(path)
            .map(|bytes| u64::try_from(bytes.len()).expect("a small file"))
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
pub(crate) fn rows(findings: &[Finding]) -> Vec<String> {
    let mut rows: Vec<String> = findings
        .iter()
        .map(|finding| {
            let (path, line) = match &finding.location {
                Some(gate_sdk::Location::File(location)) => (
                    location.path.clone(),
                    location.span.map_or(0, |span| span.line),
                ),
                _ => (String::new(), 0),
            };
            format!(
                "{} {path}:{line} {}",
                finding.aspect,
                finding.key.as_deref().unwrap_or("")
            )
        })
        .collect();
    rows.sort();
    rows
}

/// Every text of every finding, for checks that a value never appears.
pub(crate) fn texts(findings: &[Finding]) -> String {
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

#[test]
fn both_rules_read_the_repository() {
    let info = gate_sdk::describe(&repository::PLUGIN);
    let names: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, ["tracked-secrets", "large-files"]);
    assert!(
        info.rules
            .iter()
            .all(|rule| rule.scope == Scope::Repository)
    );
}

#[test]
fn each_preset_enables_exactly_the_plugins_rules_at_its_level() {
    let info = gate_sdk::describe(&repository::PLUGIN);
    let mut presets = BTreeMap::new();
    for preset in &info.presets {
        let rules: Value = serde_json::from_str(&preset.rules).expect("the preset is JSON");
        let mut levels = Vec::new();
        for (rule, settings) in rules.as_object().expect("rules by name") {
            let declared = info
                .rules
                .iter()
                .find(|info| info.name == *rule)
                .unwrap_or_else(|| panic!("{} names the unknown rule {rule}", preset.name));
            levels.push(format!(
                "{rule} {}",
                settings["level"].as_str().unwrap_or("")
            ));
            for (aspect, level) in settings["levels"].as_object().into_iter().flatten() {
                assert!(
                    declared.aspects.contains(aspect),
                    "{} sets {rule}#{aspect}, which the rule does not declare",
                    preset.name
                );
                levels.push(format!("{rule}#{aspect} {}", level.as_str().unwrap_or("")));
            }
        }
        presets.insert(preset.name.clone(), levels);
    }
    assert_eq!(
        presets.keys().collect::<Vec<_>>(),
        ["adoption", "recommended"]
    );
    assert_eq!(
        presets["recommended"],
        [
            "tracked-secrets error",
            "large-files error",
            "large-files#large warning"
        ]
    );
    assert_eq!(
        presets["adoption"],
        ["tracked-secrets warning", "large-files warning"]
    );
    for levels in presets.values() {
        let rules: Vec<&str> = levels
            .iter()
            .filter(|level| !level.contains('#'))
            .map(|level| level.split(' ').next().unwrap_or(""))
            .collect();
        let declared: Vec<&str> = info.rules.iter().map(|rule| rule.name.as_str()).collect();
        assert_eq!(rules, declared);
    }
}

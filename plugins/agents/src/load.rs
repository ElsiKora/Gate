//! The agent instructions of a project: its rule files and skills, and the options every rule
//! shares.

use std::collections::BTreeMap;

use gate_agents_format::{Rule, RuleFile, parse_rule_file};
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Report, Result, Tree};

/// The default of every `root` option.
pub(crate) fn default_root() -> String {
    "docs/agents".to_owned()
}

/// Options with only `root`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct RootOptions {
    /// The directory of the agent instructions, relative to the project: rule files in
    /// `<root>/rules/*.md`, skills in `<root>/skills/<name>/`.
    pub root: String,
}

impl Default for RootOptions {
    fn default() -> Self {
        Self {
            root: default_root(),
        }
    }
}

/// One skill directory and its tracked files.
#[derive(Debug)]
pub(crate) struct Skill {
    pub(crate) name: String,
    pub(crate) directory: String,
    pub(crate) files: Vec<String>,
}

/// The rule files and skills under a root.
#[derive(Debug)]
pub(crate) struct Agents {
    pub(crate) root: String,
    pub(crate) rules: Vec<RuleFile>,
    pub(crate) skills: Vec<Skill>,
}

impl Agents {
    /// Reads `<root>/rules/*.md` (direct children) and `<root>/skills/<name>/**`.
    ///
    /// # Errors
    /// When a rule file is not UTF-8 text.
    pub(crate) fn load(tree: &Tree<'_>, root: &str) -> Result<Self> {
        let root = root.trim_end_matches('/').to_owned();
        let rules_dir = format!("{root}/rules");
        let skills_dir = format!("{root}/skills");
        let mut rules = Vec::new();

        for path in tree.under(&rules_dir) {
            let relative = &path[rules_dir.len() + 1..];
            if relative.contains('/') || !has_extension(relative, "md") {
                continue;
            }
            let text = strict_text(tree, &path)?.unwrap_or_default();
            rules.push(parse_rule_file(&path, &text));
        }

        let mut skills: BTreeMap<String, Skill> = BTreeMap::new();
        for path in tree.under(&skills_dir) {
            let relative = &path[skills_dir.len() + 1..];
            let Some((name, _)) = relative.split_once('/') else {
                continue;
            };
            skills
                .entry(name.to_owned())
                .or_insert_with(|| Skill {
                    name: name.to_owned(),
                    directory: format!("{skills_dir}/{name}"),
                    files: Vec::new(),
                })
                .files
                .push(path);
        }

        Ok(Self {
            root,
            rules,
            skills: skills.into_values().collect(),
        })
    }

    /// Every rule by id; the first definition wins.
    pub(crate) fn registry(&self) -> BTreeMap<&str, (&RuleFile, &Rule)> {
        let mut registry = BTreeMap::new();
        for file in &self.rules {
            for rule in &file.rules {
                registry.entry(rule.id.as_str()).or_insert((file, rule));
            }
        }
        registry
    }

    pub(crate) fn rules_dir(&self) -> String {
        format!("{}/rules", self.root)
    }

    /// Reports, as `<what>#input`, a rules directory without a rule file; whether it has one.
    /// `what` is `agents/<rule>` for a rule and `agents/<generator>` for a generator, whose
    /// findings carry no aspect.
    pub(crate) fn has_rules(&self, report: &mut Report, what: &str, aspect: &str) -> bool {
        if !self.rules.is_empty() {
            return true;
        }
        let directory = self.rules_dir();
        report
            .finding(
                aspect,
                Location::file(directory.clone()),
                format!(
                    "{directory} has no rule file, and {what} reads the rule files there (its option root); create a rule file {directory}/<area>.md, point root at the agent docs, or {}",
                    turn_off(what)
                ),
            )
            .target(directory.clone())
            .key(directory);
        false
    }

    pub(crate) fn skills_dir(&self) -> String {
        format!("{}/skills", self.root)
    }
}

/// Whether `path` has the extension `extension`, compared without regard to case.
pub(crate) fn has_extension(path: &str, extension: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|found| found.to_str())
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// The text of a tracked file that must be UTF-8.
///
/// # Errors
/// When the file is not UTF-8 text.
pub(crate) fn strict_text(tree: &Tree<'_>, path: &str) -> Result<Option<String>> {
    tree.bytes(path)
        .map(|bytes| {
            String::from_utf8(bytes)
                .map_err(|_| Error::new(format!("{path} is not UTF-8 text; save it as UTF-8")))
        })
        .transpose()
}

/// The aspect every agents rule that reads a file has: the file, or the rule files, are missing,
/// so the rule cannot run. `levels = { input = "off" }` or an exception is the explicit way out.
pub(crate) const INPUT: &str = "input";

/// How a message says to turn the input off for `what` (`agents/<rule>` or `agents/<generator>`).
fn turn_off(what: &str) -> String {
    if what == "agents/agents-block" {
        "remove the generator from [generate]".to_owned()
    } else {
        format!(
            "turn the input off with a decision: \"{what}\" = {{ levels = {{ input = \"off\" }}, decision = \"<record>\" }}"
        )
    }
}

/// Reports, as `<what>#input`, the entry file `entry` when `tree` does not track it; whether it is
/// tracked. `create` says how to create it: `"create it"`, or with what it must hold.
pub(crate) fn has_entry(
    report: &mut Report,
    tree: &Tree<'_>,
    entry: &str,
    what: &str,
    aspect: &str,
    create: &str,
) -> bool {
    if tree.contains(entry) {
        return true;
    }
    report
        .finding(
            aspect,
            Location::file(entry),
            format!(
                "{entry} is not tracked, and {what} reads it (its option entry); {create}, point entry at the file, or {}",
                turn_off(what)
            ),
        )
        .target(entry)
        .key(entry);
    false
}

/// A location at `line` of `path`, or the whole file without one.
pub(crate) fn at(path: &str, line: Option<usize>) -> Location {
    match line {
        Some(line) => Location::line(path, line),
        None => Location::file(path),
    }
}

//! `agents/rule-registry`: rule files declare their area, scope and owner, and every rule has a
//! level-two heading and a well-formed Enforcement/Status line.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use gate_agents_format::{CLASSES, Rule as AgentRule, RuleFile, STATUSES};
use gate_sdk::{Report, Result, Rule, Run};
use regex::Regex;

use crate::load::{Agents, INPUT, RootOptions, at};

static AREA: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9]{1,7}$").ok());

const HELP: &str = "follow the rule-file format: front matter with area, applies (and globs) and \
                    owner, then `## RULE-<AREA>-<nnn> <title>` headings, each followed by \
                    `Enforcement: <class>[ <detail>] · Status: <status>[ · Redirect: <id>]`";

/// The rule.
#[derive(Debug, Default)]
pub struct RuleRegistry;

impl Rule for RuleRegistry {
    type Options = RootOptions;

    const NAME: &'static str = "rule-registry";
    const TITLE: &'static str = "Rule registry, headings and metadata lines agree";
    const ASPECTS: &'static [&'static str] = &[INPUT];

    fn check(&self, run: &mut Run<'_, RootOptions>) -> Result<()> {
        let agents = Agents::load(&run.tree(), &run.options().root)?;
        let report = run.report();
        if !agents.has_rules(report, "agents/rule-registry", INPUT) {
            return Ok(());
        }
        let mut areas: BTreeMap<&str, &str> = BTreeMap::new();

        for file in &agents.rules {
            if !front_matter(file, &mut areas, report) {
                continue;
            }
            for rule in &file.rules {
                rule_lines(file, rule, report);
            }
            for line in &file.stray_metadata {
                error(
                    report,
                    file,
                    Some(*line),
                    "an Enforcement line follows no rule heading",
                );
            }
        }
        Ok(())
    }
}

fn error(report: &mut Report, file: &RuleFile, line: Option<usize>, message: impl Into<String>) {
    report
        .finding("", at(&file.path, line), message)
        .target(file.path.clone())
        .help(HELP);
}

/// [`error`] with a key, for a message that embeds a path or a parser position.
fn error_keyed(
    report: &mut Report,
    file: &RuleFile,
    line: Option<usize>,
    message: impl Into<String>,
    key: &str,
) {
    report
        .finding("", at(&file.path, line), message)
        .target(file.path.clone())
        .help(HELP)
        .key(key);
}

/// Checks the front matter; `false` when the rest of the file cannot be judged.
fn front_matter<'a>(
    file: &'a RuleFile,
    areas: &mut BTreeMap<&'a str, &'a str>,
    report: &mut Report,
) -> bool {
    if let Some(problem) = &file.frontmatter_error {
        error_keyed(
            report,
            file,
            Some(1),
            format!("frontmatter is not valid YAML: {problem}"),
            "frontmatter#yaml",
        );
        return false;
    }
    if file.frontmatter.is_none() {
        error(
            report,
            file,
            Some(1),
            "a rule file starts with frontmatter that declares area, applies and owner",
        );
        return false;
    }
    let line = |name: &str| file.field(name).map(|node| node.line).or(Some(1));

    match file.area() {
        Some(area) if AREA.as_ref().is_some_and(|pattern| pattern.is_match(area)) => {
            if let Some(other) = areas.insert(area, &file.path) {
                error_keyed(
                    report,
                    file,
                    line("area"),
                    format!("area {area} is already declared by {other}"),
                    area,
                );
            }
        }
        Some(area) => error(
            report,
            file,
            line("area"),
            format!("area {area} is not 2-8 uppercase letters or digits"),
        ),
        None => error(report, file, Some(1), "frontmatter has no area"),
    }

    match (file.applies(), file.globs().is_empty()) {
        (Some("always"), true) | (Some("globs"), false) => {}
        (Some("always"), false) => error(
            report,
            file,
            line("globs"),
            "an always-applying rule file declares no globs",
        ),
        (Some("globs"), true) => error(
            report,
            file,
            line("applies"),
            "applies: globs needs a non-empty globs list",
        ),
        (Some(other), _) => error(
            report,
            file,
            line("applies"),
            format!("applies is `{other}`, not `always` or `globs`"),
        ),
        (None, _) => error(report, file, Some(1), "frontmatter has no applies"),
    }
    true
}

fn rule_lines(file: &RuleFile, rule: &AgentRule, report: &mut Report) {
    if rule.heading_level != 2 {
        error(
            report,
            file,
            Some(rule.line),
            format!(
                "{} is a level-{} heading; rule headings are level 2",
                rule.id, rule.heading_level
            ),
        );
    }
    if let (Some(rule_area), Some(file_area)) = (rule.area(), file.area())
        && rule_area != file_area
    {
        error(
            report,
            file,
            Some(rule.line),
            format!(
                "{} belongs to area {rule_area}, but the file declares {file_area}",
                rule.id
            ),
        );
    }
    match rule.parse_metadata() {
        Err(message) => {
            let line = rule.metadata.as_ref().map_or(rule.line, |(line, _)| *line);
            error(report, file, Some(line), message);
        }
        Ok(metadata) => {
            let line = rule.metadata.as_ref().map(|(line, _)| *line);
            if !CLASSES.contains(&metadata.class.as_str()) {
                error(
                    report,
                    file,
                    line,
                    format!(
                        "{} has unknown enforcement class `{}`",
                        rule.id, metadata.class
                    ),
                );
            }
            if !STATUSES.contains(&metadata.status.as_str()) {
                error(
                    report,
                    file,
                    line,
                    format!("{} has unknown status `{}`", rule.id, metadata.status),
                );
            }
            if metadata.redirect.is_some()
                && !matches!(metadata.status.as_str(), "removed" | "deprecated")
            {
                error(
                    report,
                    file,
                    line,
                    format!(
                        "{} has a redirect but status `{}`",
                        rule.id, metadata.status
                    ),
                );
            }
        }
    }
}

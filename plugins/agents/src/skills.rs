//! `agents/skills`: every skill follows the Agent Skills specification.

use std::sync::LazyLock;

use gate_agents_format::markdown::frontmatter;
use gate_agents_format::yaml::Node;
use gate_sdk::{Result, Rule, Run};
use regex::Regex;

use crate::load::{Agents, RootOptions, at, strict_text};

static SKILL_NAME: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").ok());

/// The front matter fields the specification defines.
const FIELDS: [&str; 6] = [
    "name",
    "description",
    "license",
    "compatibility",
    "metadata",
    "allowed-tools",
];

/// The rule.
#[derive(Debug, Default)]
pub struct Skills;

/// The findings of one run: path, line and message.
type Found = Vec<(String, Option<usize>, String)>;

impl Rule for Skills {
    type Options = RootOptions;

    const NAME: &'static str = "skills";
    const TITLE: &'static str = "Skills follow the Agent Skills specification";

    fn check(&self, run: &mut Run<'_, RootOptions>) -> Result<()> {
        let tree = run.tree();
        let agents = Agents::load(&tree, &run.options().root)?;
        let mut found: Found = Vec::new();

        for skill in &agents.skills {
            let manifest = format!("{}/SKILL.md", skill.directory);
            let Some(text) = strict_text(&tree, &manifest)? else {
                found.push((
                    skill.directory.clone(),
                    None,
                    "a skill directory needs SKILL.md".to_owned(),
                ));
                continue;
            };
            let block = match frontmatter(&text) {
                Ok(Some(block)) => block,
                Ok(None) => {
                    found.push((
                        manifest,
                        Some(1),
                        "SKILL.md starts with YAML frontmatter".to_owned(),
                    ));
                    continue;
                }
                Err(error) => {
                    found.push((
                        manifest,
                        Some(1),
                        format!("frontmatter is not valid YAML: {error}"),
                    ));
                    continue;
                }
            };
            let Some(entries) = block.data.mapping() else {
                found.push((manifest, Some(1), "frontmatter is not a mapping".to_owned()));
                continue;
            };
            for (key, _) in entries {
                let name = key.scalar().unwrap_or("");
                if !FIELDS.contains(&name) {
                    found.push((
                        manifest.clone(),
                        Some(key.line),
                        format!("`{name}` is not an Agent Skills field"),
                    ));
                }
            }
            fields(&skill.name, &block.data, &manifest, &mut found);
        }

        for (path, line, message) in found {
            run.report()
                .finding("", at(&path, line), message)
                .target(path.clone())
                .help(
                    "write SKILL.md as the Agent Skills specification says: front matter with a \
                     name equal to the directory and a description",
                );
        }
        Ok(())
    }
}

fn fields(directory: &str, data: &Node, manifest: &str, found: &mut Found) {
    let mut push = |line: Option<usize>, message: String| {
        found.push((manifest.to_owned(), line, message));
    };

    match data.get("name").and_then(Node::scalar) {
        None => push(Some(1), "name is required".to_owned()),
        Some(name) => {
            let line = data.get("name").map(|node| node.line);
            let well_formed = SKILL_NAME
                .as_ref()
                .is_some_and(|pattern| pattern.is_match(name));
            if !well_formed || name.chars().count() > 64 {
                push(
                    line,
                    "name is 1-64 lowercase letters, digits and single hyphens".to_owned(),
                );
            }
            if name != directory {
                push(
                    line,
                    format!("name `{name}` differs from the directory `{directory}`"),
                );
            }
        }
    }

    match data.get("description").and_then(Node::scalar) {
        Some(description)
            if !description.trim().is_empty() && description.chars().count() <= 1024 => {}
        _ => push(
            data.get("description").map(|node| node.line).or(Some(1)),
            "description is required and has 1-1024 characters".to_owned(),
        ),
    }

    if let Some(node) = data.get("compatibility")
        && node
            .scalar()
            .is_none_or(|value| value.chars().count() > 500)
    {
        push(
            Some(node.line),
            "compatibility is a string of at most 500 characters".to_owned(),
        );
    }

    if let Some(node) = data.get("metadata")
        && !node.mapping().is_some_and(|entries| {
            entries
                .iter()
                .all(|(key, value)| key.scalar().is_some() && value.scalar().is_some())
        })
    {
        push(
            Some(node.line),
            "metadata maps strings to strings".to_owned(),
        );
    }

    for field in ["license", "allowed-tools"] {
        if let Some(node) = data.get(field)
            && node.scalar().is_none()
        {
            push(Some(node.line), format!("{field} is a string"));
        }
    }
}

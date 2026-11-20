//! `agents/agents-block`: the managed block of the entry file (`AGENTS.md`) that lists the rule
//! files.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Generate, Generator, Location, Result};

use crate::block;
use crate::load::{Agents, at, default_root, strict_text};

/// The name of the managed block.
const BLOCK: &str = "agents";

/// The generator.
#[derive(Debug, Default)]
pub struct AgentsBlock;

/// The options of `agents/agents-block`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directory of the agent instructions, relative to the project.
    pub root: String,
    /// The file that holds the block, relative to the project, written plainly (no leading `/`,
    /// no `.`, `..` or empty segment). The generator writes only between the markers; while the
    /// file is not tracked, it seeds it: a title, one line, and the block.
    pub entry: String,
    /// The command the block names as the one that writes it.
    pub command_text: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            root: default_root(),
            entry: "AGENTS.md".to_owned(),
            command_text: "bin/gate sync".to_owned(),
        }
    }
}

impl Generator for AgentsBlock {
    type Options = Options;

    const NAME: &'static str = "agents-block";
    const TITLE: &'static str = "The rule-file table in the managed block of AGENTS.md";

    fn generate(&self, run: &mut Generate<'_, Options>) -> Result<()> {
        let tree = run.tree();
        let options = run.options();
        let entry = plain(&options.entry)?;
        let command = options.command_text.clone();
        let agents = Agents::load(&tree, &options.root)?;
        // A block with no rule file lists nothing: a finding of `gate sync`, never a silent pass.
        let has_rules = agents.has_rules(run.report(), "agents/agents-block", "");
        // With nothing tracked under the root, the root's reading map is seeded, so the agents'
        // area of the book exists after `gate sync`; the rule files stay the person's to write.
        if tree.under(&agents.root).is_empty() {
            run.seed(format!("{}/{CONTEXT_MAP}", agents.root), CONTEXT_MAP_SEED);
        }
        // An entry that is not tracked is seeded; `gate sync` writes it only where nothing exists,
        // so a file a person wrote and has not added yet is left alone.
        if !tree.contains(&entry) {
            run.seed(entry.clone(), seed(&inner(&agents, &entry, &command)));
            return Ok(());
        }
        let Some(text) = strict_text(&tree, &entry)? else {
            return Ok(());
        };
        if !has_rules && !block::has_marker(&text, BLOCK) {
            return Ok(());
        }
        let inner = inner(&agents, &entry, &command);

        match block::replace(&text, BLOCK, &inner) {
            Ok(contents) => run.write(entry.clone(), contents),
            Err((line, message)) => {
                let location: Location = at(&entry, line);
                run.report()
                    .finding("", location, message)
                    .target(entry.clone())
                    .help(format!(
                        "put one `<!-- gate:begin {BLOCK} -->` line and, below it, one \
                         `<!-- gate:end {BLOCK} -->` line in {entry}"
                    ));
            }
        }
        Ok(())
    }
}

/// The reading map of the agent instructions, in the root.
const CONTEXT_MAP: &str = "context-map.md";

/// The seed of the reading map: a title and the line that says what goes there.
const CONTEXT_MAP_SEED: &str = "# Context map\n\nThe reading map for coding agents in this project: which pages to read before which kind of change.\n";

/// The seed of the entry: a title, one line on what the file is, and the block holding `inner`,
/// byte for byte what the generator writes into the seeded file afterwards.
fn seed(inner: &str) -> String {
    let separator = if inner.is_empty() { "" } else { "\n" };
    format!(
        "# Agent instructions\n\nThe entry point for coding agents in this project: the rule files below \
         apply to their areas.\n\n<!-- gate:begin {BLOCK} -->\n\n{inner}{separator}<!-- gate:end {BLOCK} -->\n"
    )
}

/// The text between the markers; links are relative to `entry`, the file that holds the block.
fn inner(agents: &Agents, entry: &str, command: &str) -> String {
    if agents.rules.is_empty() {
        return String::new();
    }
    let mut rows = Vec::new();
    let mut always = Vec::new();
    for file in &agents.rules {
        let link = format!(
            "[{}]({})",
            escape_cell(file.title().unwrap_or(&file.stem)),
            relative(entry, &file.path)
        );
        let applies = match file.applies() {
            Some("globs") => file
                .globs()
                .iter()
                .map(|glob| format!("`{}`", escape_cell(glob)))
                .collect::<Vec<_>>()
                .join(", "),
            Some(other) => other.to_owned(),
            None => String::new(),
        };
        rows.push(vec![
            format!("`{}`", file.area().unwrap_or("")),
            link.clone(),
            applies,
            format!("`{}`", file.owner().unwrap_or("")),
        ]);
        if file.applies() == Some("always") {
            always.push(format!("- {link}"));
        }
    }

    let mut inner = format!(
        "Generated by `{command}` from `{}`; edit the rule files, not this block.\n\n",
        agents.rules_dir()
    );
    inner.push_str(&block::table(&["Area", "Rules", "Applies", "Owner"], &rows));
    if !always.is_empty() {
        inner.push_str("\nAlways-apply rules:\n\n");
        inner.push_str(&always.join("\n"));
        inner.push('\n');
    }
    inner
}

/// `entry` when it is a plain project-relative path, the form the block's links are computed from.
///
/// # Errors
/// When it is empty, starts with `/`, or has a `.`, `..` or empty segment.
fn plain(entry: &str) -> Result<String> {
    if entry
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::config(format!(
            "agents/agents-block option entry = {entry:?} is not a plain project-relative path; \
             write it without a leading `/` and without `.`, `..` or empty segments, such as \
             \"AGENTS.md\" or \"docs/AGENTS.md\""
        )));
    }
    Ok(entry.to_owned())
}

/// `text` with every `|` escaped, so it stays inside one table cell.
fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The path of `target` relative to the directory of the file `from`; both are project-relative.
fn relative(from: &str, target: &str) -> String {
    let directory: Vec<&str> = from.split('/').collect();
    let directory = &directory[..directory.len().saturating_sub(1)];
    let target_parts: Vec<&str> = target.split('/').collect();
    let common = directory
        .iter()
        .zip(&target_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts: Vec<&str> = vec![".."; directory.len() - common];
    parts.extend_from_slice(&target_parts[common..]);
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::{escape_cell, plain, relative};

    #[test]
    fn an_entry_that_is_not_a_plain_path_is_refused() {
        assert_eq!(
            plain("docs/AGENTS.md").ok().as_deref(),
            Some("docs/AGENTS.md")
        );
        for entry in [
            "./AGENTS.md",
            "/AGENTS.md",
            "docs/../AGENTS.md",
            "docs//AGENTS.md",
            "",
        ] {
            let error = plain(entry).expect_err(entry);
            assert!(
                error
                    .message()
                    .contains("not a plain project-relative path"),
                "{error}"
            );
        }
    }

    #[test]
    fn links_are_relative_to_the_entry_file() {
        assert_eq!(
            relative("AGENTS.md", "docs/agents/rules/api.md"),
            "docs/agents/rules/api.md"
        );
        assert_eq!(
            relative("docs/AGENTS.md", "docs/agents/rules/api.md"),
            "agents/rules/api.md"
        );
        assert_eq!(
            relative("guide/AGENTS.md", "docs/agents/rules/api.md"),
            "../docs/agents/rules/api.md"
        );
    }

    #[test]
    fn a_pipe_is_escaped_in_a_cell() {
        assert_eq!(escape_cell("a | b"), "a \\| b");
    }
}

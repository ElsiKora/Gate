//! `markdown/required-sections`: a documentation page whose path matches an entry of `sections`
//! has each level-two heading the entry names.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};
use regex::Regex;

use crate::pages::{default_roots, pages};
use crate::text::{blank_fences_and_comments, has_line_terminator};

/// The rule.
#[derive(Debug, Default)]
pub struct RequiredSections;

/// The options of `markdown/required-sections`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directories, relative to the project, whose tracked `.md` files are checked; an empty
    /// string is the whole project.
    pub roots: Vec<String>,
    /// Further tracked files to check, whatever their extension; a file that is not tracked is
    /// skipped.
    pub extra_files: Vec<String>,
    /// The pages and the sections each must have; the first entry whose `path` matches a page
    /// decides. Empty: the rule reports nothing.
    pub sections: Vec<Sections>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            roots: default_roots(),
            extra_files: Vec::new(),
            sections: Vec::new(),
        }
    }
}

/// One entry of `sections`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Sections {
    /// A regular expression (Rust `regex` syntax) that must match the page's whole path, relative
    /// to the project.
    pub path: String,
    /// The texts of the `##` headings the page must have.
    pub sections: Vec<String>,
}

impl Rule for RequiredSections {
    type Options = Options;

    const NAME: &'static str = "required-sections";
    const TITLE: &'static str =
        "Documentation pages have the level-two sections their path requires";

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let entries = compiled(&run.options().sections)?;
        if entries.is_empty() {
            return Ok(());
        }
        let tree = run.tree();
        let mut missing = Vec::new();

        for page in pages(&tree, &run.options().roots, &run.options().extra_files) {
            let Some((_, sections)) = entries.iter().find(|(pattern, _)| pattern.is_match(&page))
            else {
                continue;
            };
            let Some(text) = tree.text(&page) else {
                continue;
            };
            let headings = level_two_headings(&text);
            for section in *sections {
                if !headings.contains(section) {
                    missing.push((page.clone(), section.clone()));
                }
            }
        }

        for (page, section) in missing {
            run.report()
                .finding(
                    "",
                    Location::file(&page),
                    format!("{page} is missing module documentation section: {section}"),
                )
                .target(format!("{page}:{section}"))
                .help(format!(
                    "add a `## {section}` heading with its content, outside code fences and HTML \
                     comments"
                ));
        }

        Ok(())
    }
}

/// The entries with their patterns anchored to the whole path, in order. A pattern must compile
/// on its own too, so that it cannot close the anchoring group.
fn compiled(entries: &[Sections]) -> gate_sdk::Result<Vec<(Regex, &[String])>> {
    entries
        .iter()
        .map(|entry| {
            Regex::new(&entry.path)
                .and_then(|_| Regex::new(&format!("^(?:{})$", entry.path)))
                .map(|pattern| (pattern, entry.sections.as_slice()))
                .map_err(|error| {
                    gate_sdk::Error::new(format!(
                        "sections has an invalid path pattern {:?}: {error}; write a Rust regular \
                         expression that matches a page's whole path",
                        entry.path
                    ))
                })
        })
        .collect()
}

/// The texts of the `##` headings outside code fences and HTML comments, each line read as
/// `^ {0,3}##[\t ]+(.+?)(?:[\t ]+#+)?[\t ]*$`.
fn level_two_headings(text: &str) -> Vec<String> {
    blank_fences_and_comments(text)
        .split('\n')
        .filter_map(heading)
        .collect()
}

fn heading(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 || has_line_terminator(line) {
        return None;
    }
    let rest = line[indent..].strip_prefix("##")?;
    let content = rest.trim_start_matches([' ', '\t']);
    if content.len() == rest.len() {
        return None;
    }
    let mut text = content.trim_end_matches([' ', '\t']);
    let unhashed = text.trim_end_matches('#');
    if unhashed.len() < text.len() {
        let before = unhashed.trim_end_matches([' ', '\t']);
        if before.len() < unhashed.len() && !before.is_empty() {
            text = before;
        }
    }
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::heading;

    #[test]
    fn closing_hashes_drop_only_after_white_space() {
        assert_eq!(heading("## Source Map ##").as_deref(), Some("Source Map"));
        assert_eq!(heading("   ##\tLinks  ").as_deref(), Some("Links"));
        assert_eq!(heading("## C#").as_deref(), Some("C#"));
        assert_eq!(heading("## # ").as_deref(), Some("#"));
        assert_eq!(heading("    ## Indented"), None);
        assert_eq!(heading("##Missing space"), None);
        assert_eq!(heading("### Deeper"), None);
        assert_eq!(heading("## Carriage\r"), None);
    }
}

//! `markdown/frontmatter`: documentation pages open with a front matter block that holds each
//! required key once, with a value.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};

use crate::pages::{default_roots, pages};
use crate::text::{has_line_terminator, is_js_space};

/// The rule.
#[derive(Debug, Default)]
pub struct Frontmatter;

/// The options of `markdown/frontmatter`.
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
    /// The keys the front matter must hold, each once and with a value; at least one.
    pub required_keys: Vec<String>,
    /// Path prefixes of pages the rule does not check (pages with a front matter format of their
    /// own).
    pub exclude: Vec<String>,
    /// Path prefixes of record directories: a page there needs the keys only when it opens a
    /// front matter block; its `README.md` files always do.
    pub record_directories: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            roots: default_roots(),
            extra_files: Vec::new(),
            required_keys: ["title", "description", "category", "status"]
                .map(str::to_owned)
                .to_vec(),
            exclude: vec!["docs/agents/skills/".to_owned()],
            record_directories: vec!["docs/ledger/".to_owned(), "docs/adr/".to_owned()],
        }
    }
}

impl Rule for Frontmatter {
    type Options = Options;

    const NAME: &'static str = "frontmatter";
    const TITLE: &'static str =
        "Documentation pages open with front matter that holds each required key";

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let options = run.options();
        if options.required_keys.is_empty() {
            return Err(gate_sdk::Error::config(
                "required-keys is empty; name at least one key, or turn the rule off",
            ));
        }
        let tree = run.tree();
        let keys = listed(&options.required_keys);
        let mut missing = Vec::new();

        for page in pages(&tree, &options.roots, &options.extra_files) {
            // Node reads a page that is not UTF-8 with replacement characters.
            let Some(text) = tree.text(&page) else {
                continue;
            };
            if needs_keys(options, &page, &text) && !has_keys(&text, &options.required_keys) {
                missing.push(page);
            }
        }

        for page in missing {
            run.report()
                .finding(
                    "",
                    Location::file(&page),
                    format!("{page} must have {keys} frontmatter"),
                )
                .target(page.clone())
                .help(format!(
                    "open the page with a block between two `---` lines that sets {keys}, \
                     one `key: value` line each"
                ));
        }

        Ok(())
    }
}

/// `a`, `a and b`, `a, b, and c`.
fn listed(keys: &[String]) -> String {
    match keys {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Whether the page must carry the keys: not excluded, and not a record body without a block.
fn needs_keys(options: &Options, page: &str, text: &str) -> bool {
    if options
        .exclude
        .iter()
        .any(|prefix| page.starts_with(prefix.as_str()))
    {
        return false;
    }
    let record_body = options
        .record_directories
        .iter()
        .any(|prefix| page.starts_with(prefix.as_str()))
        && !page.ends_with("/README.md");
    !record_body || text.starts_with("---\n")
}

/// Whether the leading block holds each key on exactly one line with a value. The block opens with
/// `---\n` and ends at the first `\n---\n`; a key's line is `key:`, then spaces or tabs, then a
/// value that starts with no white space and holds no line terminator.
fn has_keys(text: &str, keys: &[String]) -> bool {
    let Some(block) = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---\n").map(|end| &rest[..end]))
    else {
        return false;
    };
    let lines: Vec<&str> = block.split('\n').collect();

    keys.iter().all(|key| {
        let prefix = format!("{key}:");
        let mut matching = lines.iter().filter_map(|line| line.strip_prefix(&prefix));
        match (matching.next(), matching.next()) {
            (Some(rest), None) => has_value(rest),
            _ => false,
        }
    })
}

fn has_value(rest: &str) -> bool {
    let value = rest.trim_start_matches([' ', '\t']);
    value.len() < rest.len()
        && value
            .chars()
            .next()
            .is_some_and(|first| !is_js_space(first))
        && !has_line_terminator(value)
}

#[cfg(test)]
mod tests {
    use super::{has_keys, listed};

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn keys_are_listed_in_english() {
        assert_eq!(listed(&keys(&["a"])), "a");
        assert_eq!(listed(&keys(&["a", "b"])), "a and b");
        assert_eq!(listed(&keys(&["a", "b", "c", "d"])), "a, b, c, and d");
    }

    #[test]
    fn a_key_needs_one_line_and_a_value() {
        let wanted = keys(&["title"]);
        assert!(has_keys("---\ntitle: Page\n---\n", &wanted));
        assert!(has_keys("---\ntitle:\tPage\n---\n", &wanted));
        assert!(!has_keys("---\ntitle:Page\n---\n", &wanted));
        assert!(!has_keys("---\ntitle: \n---\n", &wanted));
        assert!(!has_keys("---\ntitle:  \u{a0}x\n---\n", &wanted));
        assert!(!has_keys("---\ntitle: a\ntitle: b\n---\n", &wanted));
        assert!(!has_keys("---\ntitle: a\r\n---\n", &wanted));
        assert!(!has_keys("---\ntitle: a\n", &wanted));
        assert!(!has_keys("title: a\n---\n", &wanted));
        assert!(!has_keys("---\n title: a\n---\n", &wanted));
    }
}

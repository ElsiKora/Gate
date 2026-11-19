//! Rule files: their rules, ids and Enforcement/Status lines.

use std::sync::LazyLock;

use regex::Regex;

use crate::markdown;
use crate::yaml::Node;

/// The enforcement classes a metadata line may name.
pub const CLASSES: [&str; 6] = ["gate", "eslint", "typecheck", "test", "review", "advisory"];
/// The statuses a metadata line may name.
pub const STATUSES: [&str; 4] = ["preview", "stable", "deprecated", "removed"];

static RULE_ID: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^RULE-([A-Z][A-Z0-9]{1,7})-(\d{3})$").ok());
static METADATA: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"^Enforcement: ([a-z]+)(?: (.+?))? · Status: ([a-z]+)(?: · Redirect: (\S+))?$").ok()
});

/// The area and number of a well-formed rule id (`RULE-API-004` is `("API", 4)`).
#[must_use]
pub fn rule_id(id: &str) -> Option<(&str, u32)> {
    let captures = RULE_ID.as_ref()?.captures(id)?;
    let area = captures.get(1)?.as_str();
    let number = captures.get(2)?.as_str().parse().ok()?;
    Some((area, number))
}

/// A parsed metadata line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    pub class: String,
    pub detail: Option<String>,
    pub status: String,
    pub redirect: Option<String>,
}

/// One rule of a rule file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    /// The first word of the heading.
    pub id: String,
    /// The rest of the heading.
    pub title: String,
    pub line: usize,
    pub heading_level: u8,
    /// The metadata line and its line number, when the next non-blank line after the heading
    /// starts with `Enforcement:`.
    pub metadata: Option<(usize, String)>,
}

impl Rule {
    /// The metadata line, parsed.
    ///
    /// # Errors
    /// A message that names the rule when the line is absent or malformed.
    pub fn parse_metadata(&self) -> Result<Metadata, String> {
        let Some((_, line)) = &self.metadata else {
            return Err(format!(
                "{} has no Enforcement/Status line after its heading",
                self.id
            ));
        };
        let captures = METADATA
            .as_ref()
            .and_then(|pattern| pattern.captures(line))
            .ok_or_else(|| {
                format!(
                    "{} metadata line is not `Enforcement: <class>[ <detail>] · Status: <status>[ · Redirect: <id>]`",
                    self.id
                )
            })?;
        let text = |index: usize| captures.get(index).map(|found| found.as_str().to_owned());
        Ok(Metadata {
            class: text(1).unwrap_or_default(),
            detail: text(2),
            status: text(3).unwrap_or_default(),
            redirect: text(4),
        })
    }

    /// The area of a well-formed id.
    #[must_use]
    pub fn area(&self) -> Option<&str> {
        rule_id(&self.id).map(|(area, _)| area)
    }

    /// The number of a well-formed id.
    #[must_use]
    pub fn number(&self) -> Option<u32> {
        rule_id(&self.id).map(|(_, number)| number)
    }
}

/// One parsed rule file.
#[derive(Debug)]
pub struct RuleFile {
    pub path: String,
    /// The file name without its one `.md` extension (matched case-insensitively): `a.md.md` has
    /// the stem `a.md`.
    pub stem: String,
    pub frontmatter: Option<Node>,
    /// The YAML error of a front matter block that does not parse.
    pub frontmatter_error: Option<String>,
    pub line_count: usize,
    pub rules: Vec<Rule>,
    /// Lines that start with `Enforcement:` outside fences and do not follow a rule heading.
    pub stray_metadata: Vec<usize>,
}

impl RuleFile {
    /// A front matter field.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Node> {
        self.frontmatter.as_ref().and_then(|data| data.get(name))
    }

    #[must_use]
    pub fn area(&self) -> Option<&str> {
        self.field("area").and_then(Node::scalar)
    }

    #[must_use]
    pub fn applies(&self) -> Option<&str> {
        self.field("applies").and_then(Node::scalar)
    }

    #[must_use]
    pub fn globs(&self) -> Vec<&str> {
        self.field("globs").map(Node::strings).unwrap_or_default()
    }

    #[must_use]
    pub fn owner(&self) -> Option<&str> {
        self.field("owner").and_then(Node::scalar)
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.field("title").and_then(Node::scalar)
    }
}

/// A file name without its one `.md` extension, matched case-insensitively.
fn stem(name: &str) -> &str {
    let cut = name.len().saturating_sub(3);
    match (name.get(..cut), name.get(cut..)) {
        (Some(stem), Some(extension)) if extension.eq_ignore_ascii_case(".md") => stem,
        _ => name,
    }
}

/// Parses the rule file at `path`.
#[must_use]
pub fn parse_rule_file(path: &str, text: &str) -> RuleFile {
    let (frontmatter, frontmatter_error) = match markdown::frontmatter(text) {
        Ok(block) => (block.map(|block| block.data), None),
        Err(error) => (None, Some(error)),
    };
    let lines: Vec<&str> = text.lines().collect();
    let fenced = markdown::fenced_lines(text);
    let mut rules = Vec::new();
    let mut claimed = Vec::new();

    for heading in markdown::headings(text) {
        let Some(first) = heading.text.split_whitespace().next() else {
            continue;
        };
        if !first.starts_with("RULE-") {
            continue;
        }
        let metadata = lines
            .iter()
            .enumerate()
            .skip(heading.line)
            .find(|(_, line)| !line.trim().is_empty())
            .filter(|(_, line)| line.starts_with("Enforcement:"))
            .map(|(index, line)| (index + 1, (*line).to_owned()));
        claimed.extend(metadata.as_ref().map(|(number, _)| *number));
        rules.push(Rule {
            id: first.to_owned(),
            title: heading.text[first.len()..].trim().to_owned(),
            line: heading.line,
            heading_level: heading.level,
            metadata,
        });
    }

    let stray_metadata = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            line.starts_with("Enforcement:") && !fenced.get(*index).copied().unwrap_or(false)
        })
        .map(|(index, _)| index + 1)
        .filter(|number| !claimed.contains(number))
        .collect();

    RuleFile {
        path: path.to_owned(),
        stem: stem(path.rsplit('/').next().unwrap_or(path)).to_owned(),
        frontmatter,
        frontmatter_error,
        line_count: lines.len(),
        rules,
        stray_metadata,
    }
}

#[cfg(test)]
mod tests {
    use super::{Metadata, parse_rule_file, rule_id};

    const FILE: &str = "\
---
area: API
---
# API rules

## RULE-API-001 Responses are typed

Enforcement: gate api/typed · Status: stable

## RULE-API-002 Old rule
Enforcement: review · Status: removed · Redirect: RULE-API-001

## Notes

Enforcement: stray

```
Enforcement: fenced
```
";

    #[test]
    fn rules_metadata_and_stray_lines_are_read() {
        let file = parse_rule_file("docs/agents/rules/api.md", FILE);
        assert_eq!(file.stem, "api");
        assert_eq!(file.area(), Some("API"));
        let ids: Vec<&str> = file.rules.iter().map(|rule| rule.id.as_str()).collect();
        assert_eq!(ids, ["RULE-API-001", "RULE-API-002"]);
        assert_eq!(
            file.rules[0].metadata.as_ref().map(|(line, _)| *line),
            Some(8)
        );
        assert_eq!(
            file.rules[1].parse_metadata(),
            Ok(Metadata {
                class: "review".to_owned(),
                detail: None,
                status: "removed".to_owned(),
                redirect: Some("RULE-API-001".to_owned()),
            })
        );
        assert_eq!(
            file.rules[0]
                .parse_metadata()
                .map(|metadata| metadata.detail),
            Ok(Some("api/typed".to_owned()))
        );
        assert_eq!(file.stray_metadata, [15]);
        assert_eq!(rule_id("RULE-API-004"), Some(("API", 4)));
        assert_eq!(rule_id("RULE-a-004"), None);
    }

    #[test]
    fn the_stem_drops_exactly_one_md_extension_in_any_case() {
        for (path, stem) in [
            ("docs/agents/rules/api.md", "api"),
            ("docs/agents/rules/API.MD", "API"),
            ("docs/agents/rules/a.md.md", "a.md"),
            ("docs/agents/rules/a.Md", "a"),
        ] {
            assert_eq!(parse_rule_file(path, "").stem, stem, "{path}");
        }
    }
}

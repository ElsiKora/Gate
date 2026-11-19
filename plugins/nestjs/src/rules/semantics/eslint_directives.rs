//! `nestjs/eslint-directives`: `ESLint` disable directives are exact next-line exceptions.
//!
//! In every code file (the project's root files and the files below `roots`, outside
//! `[languages.typescript] skip-folders`), each `//` or `/* */` comment that mentions
//! `eslint-disable` or `eslint-enable` is checked, in this order: in test or declaration code the
//! repository's lint configuration ignores, it is dead (`dead-test-code`); a block comment is
//! reported (`next-line-only`); one after code on its line is hidden (`placement`); it must read
//! `// eslint-disable-next-line <rule-id>[, <rule-id> ...] -- <reason>` with valid, distinct rule
//! ids (`syntax`); and the next line must be code (`placement`).

use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::oxc_ast::CommentKind;
use gate_typescript::{Allocator, SourceFile, TypeScript};
use regex_automata::meta::Regex;

use super::code_files::{Universe, js_space, js_trim, js_trim_end, js_trim_start};
use crate::shared::{pattern, project};

/// The prefix of a directive, its separator before the reason, and the separator of rule ids.
const PREFIX: &str = "// eslint-disable-next-line ";
const REASON: &str = " -- ";
const SEPARATOR: &str = ", ";

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/eslint-directives`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// Path prefixes (with a final `/`) whose files are read, besides the project's root files.
    pub roots: Vec<String>,
    /// The extensions of the files read.
    pub extensions: Vec<String>,
    /// First path segments of test code the lint configuration ignores.
    pub ignored_test_roots: Vec<String>,
    /// Path segments of test code the lint configuration ignores.
    pub ignored_test_segments: Vec<String>,
    /// A regular expression over the file name of test code the lint configuration ignores.
    pub ignored_test_file: String,
    /// A regular expression every listed rule id matches.
    pub rule_id_pattern: String,
}

impl Default for Options {
    fn default() -> Self {
        let strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();

        Self {
            roots: strings(&["infrastructure/", "scripts/", "src/", "test/"]),
            extensions: strings(&["cjs", "js", "jsx", "mjs", "ts", "tsx"]),
            ignored_test_roots: strings(&["test", "tests"]),
            ignored_test_segments: strings(&["__tests__"]),
            ignored_test_file: r"(?:^|\.)(?:e2e-spec|spec|test)\.[cm]?[jt]sx?$".to_owned(),
            rule_id_pattern: r"^(?:@[a-z0-9_-]+/)?[a-z0-9_-]+(?:/[a-z0-9_-]+)*$".to_owned(),
        }
    }
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "eslint-directives";
    const TITLE: &'static str = "ESLint directives are exact next-line exceptions";
    const ASPECTS: &'static [&'static str] =
        &["dead-test-code", "next-line-only", "placement", "syntax"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            use crate::shared::paths::Form::{Name, Prefix};
            let o = run.options();
            crate::shared::paths::check_all("roots", &o.roots, Prefix)?;
            crate::shared::paths::check_all("ignored-test-roots", &o.ignored_test_roots, Name)?;
            crate::shared::paths::check_all(
                "ignored-test-segments",
                &o.ignored_test_segments,
                Name,
            )?;
        }

        let options = run.options();
        let checker = Checker {
            options,
            test_file: pattern::compile("ignored-test-file", &options.ignored_test_file)?,
            rule_id: pattern::compile("rule-id-pattern", &options.rule_id_pattern)?,
        };
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let universe = Universe {
            roots: &options.roots,
            extensions: &options.extensions,
        };
        let mut findings = Vec::new();

        for (path, file) in universe.parsed(&typescript)? {
            for comment in &file.program().comments {
                if let Some((aspect, line, message, help)) = checker.directive(
                    &path,
                    &file,
                    comment.span.start,
                    comment.span.end,
                    comment.kind,
                ) {
                    findings.push((aspect, path.clone(), line, message, help));
                }
            }
        }

        for (aspect, path, line, message, help) in findings {
            run.report()
                .finding(
                    aspect,
                    Location::line(path, usize::try_from(line).unwrap_or(usize::MAX)),
                    message,
                )
                .help(help);
        }

        Ok(())
    }
}

/// A finding: aspect, line, message and help.
type Finding = (&'static str, u32, &'static str, &'static str);

/// The directive checks with the compiled patterns.
struct Checker<'o> {
    options: &'o Options,
    test_file: Regex,
    rule_id: Regex,
}

impl Checker<'_> {
    /// The finding of one comment that mentions an `ESLint` disable or enable directive.
    fn directive(
        &self,
        path: &str,
        file: &SourceFile<'_>,
        start: u32,
        end: u32,
        kind: CommentKind,
    ) -> Option<Finding> {
        let source = file.source();
        let text = &source[start as usize..end as usize];

        // oxc also lexes the HTML-like `<!--` and `-->` comments of scripts, which the compiler's
        // comment scan does not report.
        if !text.starts_with("//") && !text.starts_with("/*") {
            return None;
        }

        if !text.contains("eslint-disable") && !text.contains("eslint-enable") {
            return None;
        }

        let line = file.line(start);

        if self.ignored_test_code(path) {
            return Some((
                "dead-test-code",
                line,
                "is a dead ESLint directive in repository-ignored test/type code",
                "remove the directive; the lint configuration ignores this file",
            ));
        }

        if kind != CommentKind::Line {
            return Some((
                "next-line-only",
                line,
                "uses a block ESLint directive; only exact next-line directives are allowed",
                "replace it with // eslint-disable-next-line <rule-id> -- <reason> above the line",
            ));
        }

        let line_start = file
            .line_bounds(line)
            .map_or(0, |(start, _)| start as usize);

        if !js_trim(&source[line_start..start as usize]).is_empty() {
            return Some((
                "placement",
                line,
                "hides an ESLint directive after code",
                "move the directive onto its own line above the code",
            ));
        }

        let rules = directive_rules(js_trim_end(text));

        if rules.is_empty() || rules.iter().any(|rule| !self.rule_id.is_match(*rule)) {
            return Some((
                "syntax",
                line,
                "must use // eslint-disable-next-line <rule-id>[, <rule-id> ...] -- <nonempty reason>",
                "write the directive in the next-line form with a reason",
            ));
        }

        if rules.iter().collect::<HashSet<_>>().len() != rules.len() {
            return Some((
                "syntax",
                line,
                "lists duplicate ESLint rule IDs",
                "list each rule id once",
            ));
        }

        let next = line_text(file, line + 1);
        let next_start = next.map(js_trim_start);

        if next.is_none_or(|next| js_trim(next).is_empty())
            || next_start.is_some_and(|start| start.starts_with("//") || start.starts_with("/*"))
        {
            return Some((
                "placement",
                line,
                "does not target the immediate next code line",
                "put the directive directly above the code it excuses",
            ));
        }

        None
    }

    /// Whether `path` is test or declaration code the repository's lint configuration ignores.
    fn ignored_test_code(&self, path: &str) -> bool {
        let segments: Vec<&str> = path.split('/').collect();
        let name = segments.last().copied().unwrap_or_default();

        segments.first().is_some_and(|first| {
            self.options
                .ignored_test_roots
                .iter()
                .any(|root| root == first)
        }) || segments.iter().any(|segment| {
            self.options
                .ignored_test_segments
                .iter()
                .any(|ignored| ignored == segment)
        }) || self.test_file.is_match(name)
            || name.ends_with(".d.ts")
    }
}

/// The rule ids of a directive in the form
/// `// eslint-disable-next-line <rules> -- <reason>`, the rules split on `", "`; empty when the text
/// is not in that form. The reason starts and ends with a character that is not JavaScript
/// whitespace, and the rules are the shortest text before a ` -- ` that leaves such a reason.
fn directive_rules(directive: &str) -> Vec<&str> {
    let Some(rest) = directive.strip_prefix(PREFIX) else {
        return Vec::new();
    };

    for (offset, _) in rest.match_indices(REASON) {
        let rules = &rest[..offset];
        let reason = &rest[offset + REASON.len()..];

        if rules.is_empty() || rules.contains('\n') || reason.contains('\n') {
            continue;
        }

        let valid = reason.chars().next().is_some_and(|first| !js_space(first))
            && reason
                .chars()
                .next_back()
                .is_some_and(|last| !js_space(last));

        if valid {
            return rules.split(SEPARATOR).collect();
        }
    }

    Vec::new()
}

/// The text of one-based `line` without its trailing carriage returns and line feeds.
fn line_text<'f>(file: &'f SourceFile<'_>, line: u32) -> Option<&'f str> {
    let (start, end) = file.line_bounds(line)?;
    Some(file.source()[start as usize..end as usize].trim_end_matches(['\r', '\n']))
}

#[cfg(test)]
mod tests {
    use super::directive_rules;

    #[test]
    fn the_directive_form_takes_the_shortest_rules_before_a_valid_reason() {
        assert_eq!(
            directive_rules("// eslint-disable-next-line no-console -- prints the banner"),
            ["no-console"]
        );
        assert_eq!(
            directive_rules("// eslint-disable-next-line a, b --  x -- why"),
            ["a", "b --  x"]
        );
        assert_eq!(
            directive_rules("// eslint-disable-next-line a -- b -- c"),
            ["a"]
        );
        assert_eq!(
            directive_rules("// eslint-disable-next-line no-console"),
            Vec::<&str>::new()
        );
        assert_eq!(
            directive_rules("// eslint-disable-next-line  -- why"),
            Vec::<&str>::new()
        );
        assert_eq!(
            directive_rules("// eslint-disable no-console -- why"),
            Vec::<&str>::new()
        );
    }
}

//! `frontend/literal-text`: user-visible text in a `.tsx` file goes through the translation layer,
//! never as JSX text or a literal in a user-visible attribute.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{JSXAttributeName, JSXAttributeValue, JSXExpression};
use regex_automata::meta::Regex;

use crate::code::Project;
use crate::options::PACK;
use crate::rules::{Found, report};
use crate::standard::{FileName, name_of};

/// The rule.
#[derive(Debug, Default)]
pub struct LiteralText;

/// The options of `frontend/literal-text`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The attributes whose value a user reads or hears.
    pub attributes: Vec<String>,
    /// A regular expression for text that is not language, matched against the whole trimmed text.
    pub ignore: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            attributes: [
                "alt",
                "aria-label",
                "aria-description",
                "label",
                "placeholder",
                "title",
            ]
            .map(str::to_owned)
            .to_vec(),
            ignore: r"^[\d\s.,:;%/()+-]*$".to_owned(),
        }
    }
}

impl Rule<TypeScriptCall> for LiteralText {
    type Options = Options;

    const NAME: &'static str = "literal-text";
    const TITLE: &'static str = "User-visible text goes through the translation layer";
    const ASPECTS: &'static [&'static str] = &["text", "attribute"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        let ignore = Regex::new(&run.options().ignore).map_err(|error| {
            gate_sdk::Error::config(format!(
                "option ignore {:?} is no regular expression ({error}); write a pattern for text that is not language",
                run.options().ignore
            ))
        })?;
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project, &run.options().attributes, &ignore)?;
            report(run, found);
            Ok(())
        })
    }
}

/// Whether `text` is language: it has a letter and the ignore pattern does not match it.
fn language(text: &str, ignore: &Regex) -> bool {
    text.chars().any(char::is_alphabetic) && !ignore.is_match(text)
}

/// The text as a finding names it: trimmed, its white space folded, at most 40 characters.
fn shown(text: &str) -> String {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.chars().count() > 40 {
        folded.chars().take(40).collect::<String>() + "…"
    } else {
        folded
    }
}

fn check(
    project: &Project<'_, '_>,
    attributes: &[String],
    ignore: &Regex,
) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();

    for path in project.code() {
        let name = FileName::new(name_of(path));
        if name.extension != "tsx" || name.test_or_story() {
            continue;
        }
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let nodes = file.semantic().nodes();

        for node in nodes.iter() {
            let (aspect, text, start) = match node.kind() {
                AstKind::JSXText(text) => ("text", text.value.as_str(), text.span.start),
                AstKind::JSXExpressionContainer(container)
                    if !matches!(nodes.parent_kind(node.id()), AstKind::JSXAttribute(_)) =>
                {
                    match &container.expression {
                        JSXExpression::StringLiteral(literal) => {
                            ("text", literal.value.as_str(), literal.span.start)
                        }
                        _ => continue,
                    }
                }
                AstKind::JSXAttribute(attribute) => {
                    let JSXAttributeName::Identifier(name) = &attribute.name else {
                        continue;
                    };
                    if !attributes.iter().any(|known| name.name.as_str() == known) {
                        continue;
                    }
                    match &attribute.value {
                        Some(JSXAttributeValue::StringLiteral(literal)) => {
                            ("attribute", literal.value.as_str(), literal.span.start)
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };
            let trimmed = text.trim();
            if !language(trimmed, ignore) {
                continue;
            }
            let shown = shown(trimmed);
            found.push(
                Found::at(
                    aspect,
                    path,
                    format!("{path} shows the literal text \"{shown}\""),
                    "put the text in the messages of every locale and show it through the translation function",
                )
                .line(file.line(start))
                .named(&shown),
            );
        }
    }

    Ok(found)
}

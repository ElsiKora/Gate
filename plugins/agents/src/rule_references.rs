//! `agents/rule-references`: redirects and backticked rule ids in Markdown name rules that exist
//! and are not removed.

use gate_agents_format::markdown::code_spans;
use gate_agents_format::rule_id;
use gate_sdk::{Location, Result, Rule, Run};

use crate::load::{Agents, RootOptions, at, has_extension};

/// The rule.
#[derive(Debug, Default)]
pub struct RuleReferences;

impl Rule for RuleReferences {
    type Options = RootOptions;

    const NAME: &'static str = "rule-references";
    const TITLE: &'static str = "Explicit rule references resolve";

    fn check(&self, run: &mut Run<'_, RootOptions>) -> Result<()> {
        let tree = run.tree();
        let agents = Agents::load(&tree, &run.options().root)?;
        let registry = agents.registry();
        let mut findings = Vec::new();

        for file in &agents.rules {
            for rule in &file.rules {
                if let Ok(metadata) = rule.parse_metadata()
                    && let Some(target) = &metadata.redirect
                    && !registry.contains_key(target.as_str())
                {
                    let line = rule.metadata.as_ref().map(|(line, _)| *line);
                    findings.push((
                        at(&file.path, line),
                        target.clone(),
                        format!("{} redirects to unknown {target}", rule.id),
                    ));
                }
            }
        }

        for path in tree.paths() {
            if !has_extension(&path, "md") {
                continue;
            }
            // Any tracked Markdown file may cite a rule, and one that is not UTF-8 must not fail
            // the rule for the others: a file without `RULE-` is skipped before decoding, and the
            // rest is decoded with invalid bytes replaced (a rule id is ASCII).
            let Some(bytes) = tree.bytes(&path) else {
                continue;
            };
            if !bytes.windows(5).any(|window| window == b"RULE-") {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for span in code_spans(&text) {
                if rule_id(&span.text).is_none() {
                    continue;
                }
                let message = match registry.get(span.text.as_str()) {
                    None => format!("{} is not defined by any rule file", span.text),
                    Some((_, rule)) => match rule.parse_metadata() {
                        Ok(metadata) if metadata.status == "removed" => {
                            let hint = metadata
                                .redirect
                                .map_or_else(String::new, |target| format!("; use {target}"));
                            format!("{} is removed{hint}", span.text)
                        }
                        _ => continue,
                    },
                };
                findings.push((Location::line(&path, span.line), span.text.clone(), message));
            }
        }

        for (location, target, message) in findings {
            run.report()
                .finding("", location, message)
                .target(target)
                .help("name a rule that a rule file defines and that is not removed");
        }
        Ok(())
    }
}

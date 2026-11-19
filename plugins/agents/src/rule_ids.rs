//! `agents/rule-ids`: rule ids are `RULE-<AREA>-<nnn>`, defined once, and numbered without gaps.

use std::collections::{BTreeMap, BTreeSet};

use gate_agents_format::rule_id;
use gate_sdk::{Location, Result, Rule, Run};

use crate::load::{Agents, RootOptions};

/// The rule.
#[derive(Debug, Default)]
pub struct RuleIds;

impl Rule for RuleIds {
    type Options = RootOptions;

    const NAME: &'static str = "rule-ids";
    const TITLE: &'static str = "Rule identifiers are well-formed, unique and never reused";

    fn check(&self, run: &mut Run<'_, RootOptions>) -> Result<()> {
        let agents = Agents::load(&run.tree(), &run.options().root)?;
        let report = run.report();
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        let mut numbers: BTreeMap<&str, (BTreeSet<u32>, &str)> = BTreeMap::new();

        for file in &agents.rules {
            for rule in &file.rules {
                let Some((area, number)) = rule_id(&rule.id) else {
                    report
                        .finding(
                            "",
                            Location::line(&file.path, rule.line),
                            format!("{} is not RULE-<AREA>-<NNN>", rule.id),
                        )
                        .target(rule.id.clone())
                        .help("name the rule RULE-<AREA>-<nnn>: the file's area and three digits");
                    continue;
                };
                if let Some(other) = seen.insert(&rule.id, &file.path) {
                    report
                        .finding(
                            "",
                            Location::line(&file.path, rule.line),
                            format!("{} is already defined in {other}", rule.id),
                        )
                        .target(rule.id.clone())
                        .help("give the new rule the next free number of its area");
                }
                numbers
                    .entry(area)
                    .or_insert_with(|| (BTreeSet::new(), file.path.as_str()))
                    .0
                    .insert(number);
            }
        }

        for (area, (used, path)) in numbers {
            let highest = used.last().copied().unwrap_or(0);
            for missing in (1..=highest).filter(|number| !used.contains(number)) {
                report
                    .finding(
                        "",
                        Location::file(path),
                        format!(
                            "RULE-{area}-{missing:03} is missing; identifiers are never deleted or reused, so keep it with status removed"
                        ),
                    )
                    .target(format!("RULE-{area}-{missing:03}"))
                    .help("put the rule back with `Status: removed` (and a redirect, if one replaces it)");
            }
        }
        Ok(())
    }
}

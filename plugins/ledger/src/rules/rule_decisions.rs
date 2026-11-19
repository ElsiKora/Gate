//! `ledger/rule-decisions`: the decision a rule entry of the project's `gate.toml` names is
//! accepted.

use gate_sdk::{NoOptions, Rule, Run};

use crate::catalog::{At, Findings};
use crate::index::{DecisionIndex, Refusal, refusal};
use crate::settings::Settings;

/// The rule `ledger/rule-decisions`. It reads the decisions from the host
/// ([`Run::rule_decisions`]), never from `gate.toml`.
#[derive(Debug, Default)]
pub struct RuleDecisions;

impl Rule for RuleDecisions {
    type Options = NoOptions;

    const NAME: &'static str = "rule-decisions";
    const TITLE: &'static str =
        "The decision a rule entry names is an accepted decision of the index";

    fn check(&self, run: &mut Run<'_, NoOptions>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let decisions = run.rule_decisions();

        if decisions.is_empty() {
            return Ok(());
        }

        let index_path = settings.decisions.as_str();
        let index = DecisionIndex::load(&run.tree(), index_path)?;
        let mut findings = Findings::default();

        for written in decisions {
            let key = &written.key;
            let decision = &written.decision;
            let at = written.at.span.map_or_else(
                || At::file(&written.at.path),
                |span| At::line(&written.at.path, span.line),
            );

            match refusal(index.as_ref(), decision) {
                Some(Refusal::Unlisted) => findings.emit_keyed(
                    "",
                    &at,
                    format!("{key}/{decision}#unlisted"),
                    format!(
                        "the rule entry \"{key}\" names the decision {decision}, which {index_path} does not list; name an accepted decision of the index"
                    ),
                ),
                Some(Refusal::NotAccepted(status)) => findings.emit_keyed(
                    "",
                    &at,
                    format!("{key}/{decision}#not-accepted"),
                    format!(
                        "the rule entry \"{key}\" names the decision {decision}, which is `{status}`; a rule entry needs an accepted decision"
                    ),
                ),
                None => {}
            }
        }

        findings.write_all(run.report());
        Ok(())
    }
}

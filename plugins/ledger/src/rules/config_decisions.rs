//! `ledger/config-decisions`: the decision a rule entry of any `gate.toml`, or an exception of any
//! `gate.exceptions.toml`, of the workspace names is an accepted decision of the index.

use gate_sdk::{NoOptions, Rule, Run, Scope};

use crate::catalog::{At, Findings};
use crate::index::{DecisionIndex, Refusal, refusal};
use crate::settings::Settings;

/// The rule `ledger/config-decisions`. It reads the decisions from the host
/// ([`Run::rule_decisions`]), never from `gate.toml` or `gate.exceptions.toml`: the repository
/// call carries every decision the workspace's configuration writes, whether or not the root is a
/// project, and a project's call carries none, so each is judged once.
#[derive(Debug, Default)]
pub struct ConfigDecisions;

impl Rule<crate::LedgerCall> for ConfigDecisions {
    type Options = NoOptions;

    const NAME: &'static str = "config-decisions";
    const TITLE: &'static str =
        "The decision a configuration entry names is an accepted decision of the index";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions, crate::LedgerCall>) -> gate_sdk::Result<()> {
        let decisions = run.rule_decisions();

        if decisions.is_empty() {
            return Ok(());
        }

        let settings = Settings::read(run)?;
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

            // The core writes an exception's decision at its `gate.exceptions.toml`.
            let (what, needs) = if is_exceptions_file(&written.at.path) {
                (format!("the exception for {key}"), "an exception")
            } else {
                (format!("the entry for \"{key}\""), "an entry")
            };
            match refusal(index.as_ref(), decision) {
                Some(Refusal::Unlisted) => findings.emit_keyed(
                    "",
                    &at,
                    format!("{key}/{decision}#unlisted"),
                    format!(
                        "{what} names the decision {decision}, which {index_path} does not list; name an accepted decision of the index"
                    ),
                ),
                Some(Refusal::NotAccepted(status)) => findings.emit_keyed(
                    "",
                    &at,
                    format!("{key}/{decision}#not-accepted"),
                    format!(
                        "{what} names the decision {decision}, which is `{status}`; {needs} needs an accepted decision"
                    ),
                ),
                None => {}
            }
        }

        findings.write_all(run.report());
        Ok(())
    }
}

/// Whether `path`, a repository path, is a `gate.exceptions.toml`: the file the core reads a
/// scope's exceptions from.
fn is_exceptions_file(path: &str) -> bool {
    path.rsplit('/').next() == Some("gate.exceptions.toml")
}

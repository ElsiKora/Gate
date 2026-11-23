//! `ledger/blockers`: a planned record names no closed blocker, and a started record no blocker.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};

use crate::catalog::{Catalog, Findings};
use crate::input::{self, INPUT};
use crate::records::{Ledger, Record, compile};
use crate::settings::Settings;

/// The rule.
#[derive(Debug, Default)]
pub struct Blockers;

/// The options of `ledger/blockers`: none beyond `[plugins.ledger] options`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {}

impl Rule<crate::LedgerCall> for Blockers {
    type Options = Options;

    const NAME: &'static str = "blockers";
    const TITLE: &'static str =
        "A planned record names no closed blocker and a started record no blocker";
    const ASPECTS: &'static [&'static str] = &["closed-blocker", INPUT, "started-blocker"];

    fn check(&self, run: &mut Run<'_, Options, crate::LedgerCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
        let settings = Settings::read(run)?;
        let tree = run.tree();

        if !input::tracked(run.report(), &tree, Self::NAME, &input::records(&settings)) {
            return Ok(());
        }

        let ledger = shared.ledger(&settings)?;
        let identifier = compile(r"\b((?:I|V|F)-[A-Z0-9]+(?:-[A-Z0-9]+)*)\b")?;
        let none = compile(r"^none\.?$")?;
        let mut findings = Findings::default();

        for record in ledger.all() {
            let (Some(status), Some(blocked_by)) = (&record.status, record.field("Blocked by"))
            else {
                continue;
            };

            if settings.is_planned(status) {
                for captures in identifier.captures_iter(blocked_by) {
                    let id = captures.get(1).map_or("", |id| id.as_str());

                    if let Some(closed) = closed_status(&ledger, &settings, id) {
                        closed_blocker(&mut findings, record, id, closed);
                    }
                }
            } else if settings.is_started(status) && !none.is_match(blocked_by) {
                findings.emit_keyed(
                    "started-blocker",
                    &record.location,
                    record.id.clone(),
                    format!(
                        "{} {} is in progress, so its Blocked by must be none; record the remaining wait as a dated Completion dependency",
                        record.location.path, record.id
                    ),
                );
            }
        }

        findings.write_all(run.report());
        Ok(())
        })
    }
}

/// The status of a named record when that record is closed.
fn closed_status<'l>(ledger: &'l Ledger, settings: &Settings, id: &str) -> Option<&'l str> {
    let status = if id.starts_with("F-") {
        ledger.follow_ups.get(id).map(String::as_str)
    } else {
        ledger.status(id)
    }?;

    settings.is_settled(status).then_some(status)
}

fn closed_blocker(findings: &mut Findings, record: &Record, id: &str, status: &str) {
    findings.emit_keyed(
        "closed-blocker",
        &record.location,
        format!("{}/{id}", record.id),
        format!(
            "{} {} is planned, but its Blocked by names {id}, which is `{status}`; keep only open blockers there and move the context to a Dependency context field",
            record.location.path, record.id
        ),
    );
}
